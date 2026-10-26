use super::*;
use windows_sys::Win32::{
    Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDIBits, GetObjectW, SelectObject,
    },
    UI::WindowsAndMessaging::{
        CreateWindowExW, DI_NORMAL, DestroyWindow, DrawIconEx, GetIconInfo, ICONINFO, WM_GETICON,
        WS_OVERLAPPED,
    },
};

fn bitmap(icon: HICON) -> (i32, Vec<u8>) {
    unsafe {
        let mut info = ICONINFO::default();
        assert_ne!(GetIconInfo(icon, &mut info), 0);
        let mut bitmap = BITMAP::default();
        assert_ne!(
            GetObjectW(
                info.hbmColor,
                size_of::<BITMAP>() as i32,
                (&mut bitmap as *mut BITMAP).cast()
            ),
            0
        );
        assert_eq!(bitmap.bmWidth, bitmap.bmHeight);
        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: bitmap.bmWidth,
                biHeight: -bitmap.bmHeight,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0; bitmap.bmWidth as usize * bitmap.bmHeight as usize * 4];
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        assert_ne!(
            GetDIBits(
                dc,
                info.hbmColor,
                0,
                bitmap.bmHeight as u32,
                pixels.as_mut_ptr().cast(),
                &mut header,
                DIB_RGB_COLORS
            ),
            0
        );
        DeleteDC(dc);
        DeleteObject(info.hbmColor);
        DeleteObject(info.hbmMask);
        (bitmap.bmWidth, pixels)
    }
}

fn rendered(icon: HICON, size: usize) -> Vec<u8> {
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                biHeight: -(size as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut memory = std::ptr::null_mut();
        let dib = CreateDIBSection(
            dc,
            &info,
            DIB_RGB_COLORS,
            &mut memory,
            std::ptr::null_mut(),
            0,
        );
        assert!(!dib.is_null());
        let previous = SelectObject(dc, dib);
        let pixels = std::slice::from_raw_parts_mut(memory.cast::<u8>(), size * size * 4);
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[244, 230, 221, 255]);
        }
        assert_ne!(
            DrawIconEx(
                dc,
                0,
                0,
                icon,
                size as i32,
                size as i32,
                0,
                std::ptr::null_mut(),
                DI_NORMAL
            ),
            0
        );
        GdiFlush();
        let result = pixels.to_vec();
        SelectObject(dc, previous);
        DeleteObject(dib);
        DeleteDC(dc);
        result
    }
}

#[test]
fn native_resources_preserve_premultiplied_corner_colors_at_common_dpi() {
    for (dpi, small, big) in [(96, 16, 32), (120, 20, 40), (144, 24, 48), (192, 32, 64)] {
        let icons = WindowIcons::load(dpi).expect("embedded native icons");
        for (icon, expected_size) in [(icons.small.0, small), (icons.big.0, big)] {
            let (size, pixels) = bitmap(icon);
            assert_eq!(size, expected_size);
            let mut translucent = 0;
            for y in 0..size as usize {
                for x in 0..size as usize {
                    let edge = size as usize / 5;
                    if (x >= edge && x < size as usize - edge)
                        || (y >= edge && y < size as usize - edge)
                    {
                        continue;
                    }
                    let pixel = &pixels[(y * size as usize + x) * 4..][..4];
                    let alpha = u16::from(pixel[3]);
                    if alpha > 8 && alpha < 247 {
                        translucent += 1;
                        // GDI stores premultiplied BGRA. Warm pale corner pixels
                        // must not be multiplied by alpha a second time.
                        assert!(
                            u16::from(pixel[2]) * 255 >= 230 * alpha,
                            "dark corner at {size}px: {pixel:?}"
                        );
                    }
                }
            }
            assert!(translucent > 0, "smooth corners at {size}px");
            for rendered_size in [24, 32, 48] {
                let pixels = rendered(icon, rendered_size);
                for y in 0..rendered_size {
                    for x in 0..rendered_size {
                        if x > 0 && x + 1 < rendered_size && y > 0 && y + 1 < rendered_size {
                            continue;
                        }
                        let pixel = &pixels[(y * rendered_size + x) * 4..][..4];
                        // The warm plate cannot make this background darker in
                        // red. The reported black corner dots violate this.
                        assert!(
                            pixel[2] >= 218,
                            "dark perimeter: {size}px -> {rendered_size}px at {x},{y}: {pixel:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn window_uses_resource_sizes_and_reloads_them_on_dpi_changes() {
    unsafe {
        let class = windows_sys::core::w!("STATIC");
        let window = CreateWindowExW(
            0,
            class,
            std::ptr::null(),
            WS_OVERLAPPED,
            0,
            0,
            64,
            64,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        assert!(!window.is_null());
        assert!(install(window));
        let dpi = GetDpiForWindow(window).max(96);
        for (kind, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
            let icon = SendMessageW(window, WM_GETICON, kind as usize, 0) as HICON;
            assert_eq!(bitmap(icon).0, GetSystemMetricsForDpi(metric, dpi));
        }
        SendMessageW(window, WM_DPICHANGED, 192 | (192 << 16), 0);
        for (kind, size) in [(ICON_SMALL, 32), (ICON_BIG, 64)] {
            let icon = SendMessageW(window, WM_GETICON, kind as usize, 0) as HICON;
            assert_eq!(bitmap(icon).0, size);
        }
        assert_ne!(DestroyWindow(window), 0);
    }
}
