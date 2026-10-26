//! Native non-client hit testing lets Windows manage the resize gesture directly.
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    UI::{
        HiDpi::GetDpiForWindow,
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GWL_STYLE, GetWindowLongPtrW, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT,
            HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed, WM_NCDESTROY, WM_NCHITTEST,
            WS_THICKFRAME,
        },
    },
};

const SUBCLASS_ID: usize = 0x4456_5352;

pub(in crate::ui::window_chrome) fn install(window: HWND) -> bool {
    // Called on the owning UI thread with the main viewport's live HWND. The
    // static callback retains no borrowed state and removes itself on destruction.
    unsafe { SetWindowSubclass(window, Some(window_proc), SUBCLASS_ID, 0) != 0 }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    if message == WM_NCHITTEST {
        let mut rect = RECT::default();
        // These read-only calls target the live window supplied by comctl32.
        let resizable = unsafe {
            IsZoomed(window) == 0
                && GetWindowLongPtrW(window, GWL_STYLE) as u32 & WS_THICKFRAME != 0
                && GetWindowRect(window, &mut rect) != 0
        };
        if resizable {
            let point = point_from_lparam(lparam);
            let dpi = unsafe { GetDpiForWindow(window) }.max(96);
            if let Some(hit) = hit_test(rect, point, dpi) {
                return hit as LRESULT;
            }
        }
    } else if message == WM_NCDESTROY {
        // Remove this callback while the HWND is still valid, before forwarding
        // destruction through the rest of winit's subclass chain.
        unsafe {
            RemoveWindowSubclass(window, Some(window_proc), id);
        }
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

fn point_from_lparam(value: LPARAM) -> POINT {
    // Signed coordinates are necessary for monitors left of or above the primary.
    POINT {
        x: value as u16 as i16 as i32,
        y: (value as usize >> 16) as u16 as i16 as i32,
    }
}

fn hit_test(rect: RECT, point: POINT, dpi: u32) -> Option<u32> {
    if point.x < rect.left || point.x >= rect.right || point.y < rect.top || point.y >= rect.bottom
    {
        return None;
    }
    let edge = ((6 * dpi + 48) / 96) as i32;
    let corner = ((16 * dpi + 48) / 96) as i32;
    let left = point.x - rect.left;
    let right = rect.right - 1 - point.x;
    let top = point.y - rect.top;
    let bottom = rect.bottom - 1 - point.y;
    if left < corner && top < corner {
        Some(HTTOPLEFT)
    } else if right < corner && top < corner {
        Some(HTTOPRIGHT)
    } else if left < corner && bottom < corner {
        Some(HTBOTTOMLEFT)
    } else if right < corner && bottom < corner {
        Some(HTBOTTOMRIGHT)
    } else if left < edge {
        Some(HTLEFT)
    } else if right < edge {
        Some(HTRIGHT)
    } else if top < edge {
        Some(HTTOP)
    } else if bottom < edge {
        Some(HTBOTTOM)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_resize_hit_testing_covers_all_edges_at_common_dpi_and_negative_origins() {
        for dpi in [96, 120, 144, 192] {
            for (left, top) in [(0, 0), (-1920, -1080)] {
                let rect = RECT {
                    left,
                    top,
                    right: left + 1200,
                    bottom: top + 800,
                };
                for (x, y, hit) in [
                    (2, 2, HTTOPLEFT),
                    (1197, 2, HTTOPRIGHT),
                    (2, 797, HTBOTTOMLEFT),
                    (1197, 797, HTBOTTOMRIGHT),
                    (600, 2, HTTOP),
                    (600, 797, HTBOTTOM),
                    (2, 400, HTLEFT),
                    (1197, 400, HTRIGHT),
                ] {
                    assert_eq!(
                        hit_test(
                            rect,
                            POINT {
                                x: left + x,
                                y: top + y
                            },
                            dpi
                        ),
                        Some(hit)
                    );
                }
                for (x, y) in [(600, 400), (-1, 400), (1200, 400), (600, -1), (600, 800)] {
                    assert_eq!(
                        hit_test(
                            rect,
                            POINT {
                                x: left + x,
                                y: top + y
                            },
                            dpi
                        ),
                        None
                    );
                }
            }
        }
    }

    #[test]
    fn mouse_coordinates_preserve_negative_monitor_positions() {
        let value = ((-600_i16 as u16 as usize) << 16 | -1200_i16 as u16 as usize) as LPARAM;
        let point = point_from_lparam(value);
        assert_eq!((point.x, point.y), (-1200, -600));
    }
}
