//! Load native icon sizes directly instead of letting Windows rescale RGBA icons.
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            DestroyIcon, HICON, ICON_BIG, ICON_SMALL, IMAGE_ICON, LoadImageW, SM_CXICON,
            SM_CXSMICON, SendMessageW, WM_DPICHANGED, WM_NCDESTROY, WM_SETICON,
        },
    },
};

const SUBCLASS_ID: usize = 0x4456_4943;
// build.rs/winres embeds the multi-size ICO with this resource identifier.
const RESOURCE_ID: usize = 1;

struct OwnedIcon(HICON);

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        // Only LoadImageW handles owned by this module are released here.
        unsafe { DestroyIcon(self.0) };
    }
}

struct WindowIcons {
    small: OwnedIcon,
    big: OwnedIcon,
}

impl WindowIcons {
    fn load(dpi: u32) -> Option<Self> {
        let load = |metric| {
            // The module and icon resource live for the entire process. Omitting
            // LR_SHARED ensures each DPI request gets its own requested size.
            let handle = unsafe {
                let size = GetSystemMetricsForDpi(metric, dpi);
                LoadImageW(
                    GetModuleHandleW(std::ptr::null()),
                    RESOURCE_ID as *const u16,
                    IMAGE_ICON,
                    size,
                    size,
                    0,
                )
            };
            (!handle.is_null()).then_some(OwnedIcon(handle as HICON))
        };
        Some(Self {
            small: load(SM_CXSMICON)?,
            big: load(SM_CXICON)?,
        })
    }

    fn apply(&self, window: HWND) {
        // The subclass retains both handles until they are replaced or the
        // window is destroyed. Returned handles belong to winit, not this code.
        unsafe {
            SendMessageW(
                window,
                WM_SETICON,
                ICON_SMALL as usize,
                self.small.0 as isize,
            );
            SendMessageW(window, WM_SETICON, ICON_BIG as usize, self.big.0 as isize);
        }
    }
}

pub(super) fn install(window: HWND) -> bool {
    // Called on the live viewport's owning UI thread.
    let dpi = unsafe { GetDpiForWindow(window) }.max(96);
    let Some(icons) = WindowIcons::load(dpi) else {
        return false;
    };
    let state = Box::into_raw(Box::new(icons));
    // comctl32 owns this pointer's lifetime once the subclass is installed.
    if unsafe { SetWindowSubclass(window, Some(window_proc), SUBCLASS_ID, state as usize) } == 0 {
        unsafe { drop(Box::from_raw(state)) };
        return false;
    }
    unsafe { (*state).apply(window) };
    true
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    state: usize,
) -> LRESULT {
    if message == WM_NCDESTROY {
        // Forward destruction before freeing the icon handles used by this HWND.
        unsafe {
            RemoveWindowSubclass(window, Some(window_proc), id);
            let icons = Box::from_raw(state as *mut WindowIcons);
            let result = DefSubclassProc(window, message, wparam, lparam);
            drop(icons);
            return result;
        }
    }
    if message == WM_DPICHANGED {
        let dpi = (wparam & 0xffff) as u32;
        if let Some(icons) = WindowIcons::load(dpi) {
            // Install the replacements before releasing the previous pair.
            icons.apply(window);
            unsafe { *(state as *mut WindowIcons) = icons };
        }
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

#[cfg(test)]
mod tests;
