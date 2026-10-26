//! Alpha composition preserves smooth edges; native regions are a fallback.

use eframe::egui::{self, Pos2, Rect, Vec2};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT},
    Graphics::Dwm::{DWM_BB_BLURREGION, DWM_BB_ENABLE, DWM_BLURBEHIND, DwmEnableBlurBehindWindow},
    Graphics::Gdi::{
        ClientToScreen, CombineRgn, CreateEllipticRgn, CreateRectRgn, CreateRoundRectRgn,
        DeleteObject, GetMonitorInfoW, HRGN, MONITOR_DEFAULTTONEAREST, MONITORINFO,
        MonitorFromPoint, RGN_OR, SetWindowRgn,
    },
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::{
        EnumThreadWindows, GetClientRect, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
    },
};

const TITLE: &str = "DevShop 快捷入口";

pub(super) fn work_area(ctx: &egui::Context, anchor: [f32; 2]) -> Option<Rect> {
    let scale = ctx
        .input(|input| input.viewport().native_pixels_per_point)
        .unwrap_or(ctx.pixels_per_point())
        .max(0.1);
    let point = POINT {
        x: ((anchor[0] + super::BALL_SIZE / 2.0) * scale) as i32,
        y: ((anchor[1] + super::BALL_SIZE / 2.0) * scale) as i32,
    };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // The monitor is borrowed; user32 writes this correctly-sized stack value.
    if unsafe { GetMonitorInfoW(MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST), &mut info) } == 0
    {
        return None;
    }
    Some(Rect::from_min_max(
        Pos2::new(
            info.rcWork.left as f32 / scale,
            info.rcWork.top as f32 / scale,
        ),
        Pos2::new(
            info.rcWork.right as f32 / scale,
            info.rcWork.bottom as f32 / scale,
        ),
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelRect([i32; 4]);

impl PixelRect {
    fn from_points(rect: Rect, scale: f32) -> Self {
        Self([
            (rect.min.x * scale).floor() as i32,
            (rect.min.y * scale).floor() as i32,
            (rect.max.x * scale).ceil() as i32,
            (rect.max.y * scale).ceil() as i32,
        ])
    }

    fn offset(self, x: i32, y: i32) -> Self {
        let [left, top, right, bottom] = self.0;
        Self([left + x, top + y, right + x, bottom + y])
    }

    fn fits(self, width: i32, height: i32) -> bool {
        let [left, top, right, bottom] = self.0;
        left >= 0 && top >= 0 && right <= width && bottom <= height && right > left && bottom > top
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ClipKey {
    window: usize,
    dimensions: [i32; 2],
    circle: PixelRect,
    popup: Option<PixelRect>,
    corner_diameter: i32,
}

struct OwnedRegion(HRGN);

impl OwnedRegion {
    fn ellipse(rect: PixelRect) -> Option<Self> {
        let [left, top, right, bottom] = rect.0;
        // Region creation copies these integer coordinates and owns no Rust data.
        let handle = unsafe { CreateEllipticRgn(left, top, right, bottom) };
        (!handle.is_null()).then_some(Self(handle))
    }

    fn rounded(rect: PixelRect, diameter: i32) -> Option<Self> {
        let [left, top, right, bottom] = rect.0;
        let handle = unsafe { CreateRoundRectRgn(left, top, right, bottom, diameter, diameter) };
        (!handle.is_null()).then_some(Self(handle))
    }

    fn union(&mut self, other: &Self) -> bool {
        // GDI permits the destination to equal a source; both regions are live.
        unsafe { CombineRgn(self.0, self.0, other.0, RGN_OR) != 0 }
    }

    fn transfer_to_window(mut self, window: HWND) -> bool {
        // Only the same-process floating HWND reaches this function. On success
        // Windows owns the region; on failure Drop must release it here.
        if unsafe { SetWindowRgn(window, self.0, 1) } != 0 {
            self.0 = std::ptr::null_mut();
            true
        } else {
            false
        }
    }
}

impl Drop for OwnedRegion {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // This handle has not been transferred to Windows or deleted before.
            unsafe {
                DeleteObject(self.0);
            }
        }
    }
}

fn build_region(key: ClipKey) -> Option<OwnedRegion> {
    let mut region = OwnedRegion::ellipse(key.circle)?;
    if let Some(popup) = key.popup
        && !region.union(&OwnedRegion::rounded(popup, key.corner_diameter)?)
    {
        return None;
    }
    Some(region)
}

unsafe extern "system" fn find_window(window: HWND, output: LPARAM) -> i32 {
    let mut process = 0;
    // Enumeration is synchronous; output points to the caller's live HWND slot.
    unsafe {
        GetWindowThreadProcessId(window, &mut process);
    }
    if process != std::process::id() {
        return 1;
    }
    let mut title = [0u16; 64];
    let length = unsafe { GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32) };
    if length > 0
        && TITLE
            .encode_utf16()
            .eq(title[..length as usize].iter().copied())
    {
        unsafe {
            *(output as *mut HWND) = window;
        }
        return 0;
    }
    1
}

fn floating_window() -> HWND {
    let mut window = std::ptr::null_mut();
    // Limit discovery to this UI thread, then also validate PID and exact title.
    unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(find_window),
            (&mut window as *mut HWND) as LPARAM,
        );
    }
    window
}

pub(super) fn configure_transparency(ctx: &egui::Context, ball: Rect, popup: Option<(Rect, f32)>) {
    let window = floating_window();
    if window.is_null() {
        return;
    }
    let composition_id = egui::Id::new(("devshop-floating-alpha", ctx.viewport_id()));
    let composed = match ctx.data(|data| data.get_temp::<(usize, bool)>(composition_id)) {
        Some((cached, composed)) if cached == window as usize => composed,
        _ => {
            let composed = enable_alpha_composition(window);
            ctx.data_mut(|data| data.insert_temp(composition_id, (window as usize, composed)));
            composed
        }
    };
    if composed {
        return;
    }
    let mut client = RECT::default();
    let mut outer = RECT::default();
    let mut client_origin = POINT::default();
    // All outputs are valid stack values. The UI owns this live borderless HWND.
    if unsafe {
        GetClientRect(window, &mut client) == 0
            || GetWindowRect(window, &mut outer) == 0
            || ClientToScreen(window, &mut client_origin) == 0
    } {
        return;
    }
    let scale = ctx.pixels_per_point();
    let center = ball.center();
    // Keep the binary GDI clip outside the painted rim, so it cannot cut off
    // the image's partially transparent antialiasing pixels.
    let circle = PixelRect::from_points(Rect::from_center_size(center, Vec2::splat(54.0)), scale);
    let popup_pixels = popup.map(|(rect, _)| PixelRect::from_points(rect, scale));
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    // Native resize can trail the new menu layout by a frame. Wait for a matching
    // client instead of installing an off-window/empty region during transition.
    if !circle.fits(width, height) || popup_pixels.is_some_and(|rect| !rect.fits(width, height)) {
        return;
    }
    let x = client_origin.x - outer.left;
    let y = client_origin.y - outer.top;
    let key = ClipKey {
        window: window as usize,
        dimensions: [width, height],
        circle: circle.offset(x, y),
        popup: popup_pixels.map(|rect| rect.offset(x, y)),
        corner_diameter: popup
            .map(|(_, radius)| (radius * scale * 2.0).round() as i32)
            .unwrap_or(0),
    };
    let id = egui::Id::new(("devshop-native-floating-region", ctx.viewport_id()));
    if ctx.data(|data| data.get_temp::<ClipKey>(id)) == Some(key) {
        return;
    }
    if let Some(region) = build_region(key)
        && region.transfer_to_window(window)
    {
        ctx.data_mut(|data| data.insert_temp(id, key));
    }
}

fn enable_alpha_composition(window: HWND) -> bool {
    // Configure DWM after the GL surface exists. The empty blur region enables
    // per-pixel alpha without a blur effect. The HWND belongs to this UI thread.
    let empty = unsafe { CreateRectRgn(0, 0, -1, -1) };
    if empty.is_null() {
        return false;
    }
    let blur = DWM_BLURBEHIND {
        dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
        fEnable: 1,
        hRgnBlur: empty,
        fTransitionOnMaximized: 0,
    };
    // DWM copies the region; releasing it here does not affect composition.
    let result = unsafe {
        let result = DwmEnableBlurBehindWindow(window, &blur);
        DeleteObject(empty);
        if result >= 0 {
            SetWindowRgn(window, std::ptr::null_mut(), 1);
        }
        result
    };
    result >= 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Graphics::Gdi::PtInRegion;

    #[test]
    fn closed_ball_region_excludes_square_corners_and_former_badge() {
        let key = ClipKey {
            window: 0,
            dimensions: [64, 64],
            circle: PixelRect([5, 5, 59, 59]),
            popup: None,
            corner_diameter: 0,
        };
        let region = build_region(key).unwrap();
        // This is a private GDI region only; no desktop HWND is touched by tests.
        unsafe {
            assert_eq!(PtInRegion(region.0, 0, 0), 0);
            assert_eq!(PtInRegion(region.0, 63, 63), 0);
            assert_ne!(PtInRegion(region.0, 32, 32), 0);
            assert_eq!(PtInRegion(region.0, 51, 12), 0);
        }
    }

    #[test]
    fn expanded_region_keeps_popup_and_offset_ball_without_background_gap() {
        let key = ClipKey {
            window: 0,
            dimensions: [232, 278],
            circle: PixelRect([173, 219, 227, 273]),
            popup: Some(PixelRect([0, 0, 228, 200])),
            corner_diameter: 12,
        };
        let region = build_region(key).unwrap();
        unsafe {
            assert_ne!(PtInRegion(region.0, 50, 50), 0);
            assert_ne!(PtInRegion(region.0, 200, 246), 0);
            assert_eq!(PtInRegion(region.0, 50, 240), 0);
            assert_eq!(PtInRegion(region.0, 231, 277), 0);
        }
    }
}
