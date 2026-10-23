use std::path::PathBuf;
use tauri::Manager;

#[tauri::command]
pub async fn take_screenshot(app: tauri::AppHandle) -> Result<String, String> {
    if crate::state::SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("Nectar is shutting down".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || capture_screenshot(&app))
        .await
        .map_err(|e| format!("screenshot task failed: {e}"))?
}

fn capture_screenshot(app: &tauri::AppHandle) -> Result<String, String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, ShowWindow, SW_HIDE, SW_SHOWNA};

    // Extract raw handles up front — HWND is not Send, keep only isize.
    let hwnds: Vec<isize> = app
        .webview_windows()
        .values()
        .filter_map(|w| w.hwnd().ok().map(|h| h.0 as isize))
        .collect();

    // Hide only the windows that are currently visible (settings/calendar stay hidden).
    let mut hidden: Vec<isize> = Vec::new();
    unsafe {
        for &h in &hwnds {
            let hwnd = HWND(h as *mut _);
            if IsWindowVisible(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_HIDE);
                hidden.push(h);
            }
        }
    }

    // Let DWM settle so the hidden windows are really gone from the composited frame.
    std::thread::sleep(std::time::Duration::from_millis(200));

    let result = unsafe { capture_virtual_screen() };

    // Restore every return path from here on — windows come back before the Result propagates.
    unsafe {
        for &h in &hidden {
            let _ = ShowWindow(HWND(h as *mut _), SW_SHOWNA);
        }
    }

    let (width, height, pixels) = result?;

    let png = encode_png(width, height, &pixels)?;

    let dir = screenshots_dir().ok_or_else(|| "Pictures folder not found".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let filename = chrono::Local::now()
        .format("Nectar Screenshot %Y%m%d-%H%M%S.png")
        .to_string();
    let path = dir.join(filename);
    std::fs::write(&path, &png).map_err(|e| format!("failed to write {}: {e}", path.display()))?;

    if let Err(e) = set_clipboard_dib(hwnds.first().copied(), width, height, &pixels) {
        eprintln!("screenshot: clipboard set failed: {e}");
    }

    Ok(path.to_string_lossy().to_string())
}

/// Captures the entire virtual screen (all monitors) as top-down RGBA pixels.
unsafe fn capture_virtual_screen() -> Result<(i32, i32, Vec<u8>), String> {
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
        DIB_RGB_COLORS, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    if width <= 0 || height <= 0 {
        return Err("virtual screen has no area".to_string());
    }

    let hdc_screen = GetDC(None);
    let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
    let hbm_mem = CreateCompatibleBitmap(hdc_screen, width, height);
    let h_old = SelectObject(hdc_mem, hbm_mem.into());

    // Origin can be negative (monitors left of/above the primary) — BitBlt handles it.
    let mut result = Err("BitBlt failed".to_string());
    if BitBlt(hdc_mem, 0, 0, width, height, Some(hdc_screen), vx, vy, SRCCOPY | CAPTUREBLT).is_ok() {
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [windows::Win32::Graphics::Gdi::RGBQUAD::default(); 1],
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        if GetDIBits(
            hdc_mem,
            hbm_mem,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        ) != 0
        {
            // BGRA -> RGBA with full alpha
            for chunk in pixels.chunks_exact_mut(4) {
                let b = chunk[0];
                let r = chunk[2];
                chunk[0] = r;
                chunk[2] = b;
                chunk[3] = 255;
            }
            result = Ok((width, height, pixels));
        } else {
            result = Err("GetDIBits failed".to_string());
        }
    }

    SelectObject(hdc_mem, h_old);
    let _ = DeleteObject(hbm_mem.into());
    let _ = DeleteDC(hdc_mem);
    ReleaseDC(None, hdc_screen);

    result
}

fn encode_png(width: i32, height: i32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut buf = std::io::Cursor::new(Vec::new());
    image::write_buffer_with_format(
        &mut buf,
        rgba,
        width as u32,
        height as u32,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|e| format!("PNG encode failed: {e}"))?;
    Ok(buf.into_inner())
}

fn screenshots_dir() -> Option<PathBuf> {
    unsafe {
        use windows::Win32::System::Com::CoTaskMemFree;
        use windows::Win32::UI::Shell::{SHGetKnownFolderPath, FOLDERID_Pictures, KF_FLAG_CREATE};

        let pwstr = SHGetKnownFolderPath(&FOLDERID_Pictures, KF_FLAG_CREATE, None).ok()?;
        let path = pwstr.to_string().ok();
        CoTaskMemFree(Some(pwstr.0 as *const _));
        path.map(|p| PathBuf::from(p).join("Screenshots"))
    }
}

/// Puts the image on the clipboard as CF_DIB (BITMAPINFOHEADER + bottom-up BGRA pixels).
fn set_clipboard_dib(owner: Option<isize>, width: i32, height: i32, rgba: &[u8]) -> Result<(), String> {
    unsafe {
        use windows::Win32::Foundation::{HANDLE, HWND};
        use windows::Win32::Graphics::Gdi::{BITMAPINFOHEADER, BI_RGB};
        use windows::Win32::System::DataExchange::{
            CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
        };
        const CF_DIB: u32 = 8;
        use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

        let header_size = std::mem::size_of::<BITMAPINFOHEADER>();
        let pixel_size = rgba.len();
        let total = header_size + pixel_size;

        let header = BITMAPINFOHEADER {
            biSize: header_size as u32,
            biWidth: width,
            biHeight: height, // positive — clipboard DIBs are conventionally bottom-up
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: pixel_size as u32,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };

        let mut dib = vec![0u8; total];
        dib[..header_size]
            .copy_from_slice(std::slice::from_raw_parts(&header as *const _ as *const u8, header_size));
        // Our buffer is top-down RGBA; write BGRA rows in reverse with alpha in the reserved byte.
        let stride = width as usize * 4;
        for row in 0..height as usize {
            let src = &rgba[row * stride..(row + 1) * stride];
            let dst_row = height as usize - 1 - row;
            let dst =
                &mut dib[header_size + dst_row * stride..header_size + (dst_row + 1) * stride];
            for (s, d) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
                d[0] = s[2];
                d[1] = s[1];
                d[2] = s[0];
                d[3] = 255;
            }
        }

        let owner_hwnd = owner.map(|h| HWND(h as *mut _));
        OpenClipboard(owner_hwnd).map_err(|e| format!("OpenClipboard failed: {e}"))?;
        let inner = (|| -> Result<(), String> {
            unsafe {
                EmptyClipboard().map_err(|e| format!("EmptyClipboard failed: {e}"))?;
                let hglobal = GlobalAlloc(GMEM_MOVEABLE, total)
                    .map_err(|e| format!("GlobalAlloc failed: {e}"))?;
                let ptr = GlobalLock(hglobal);
                if ptr.is_null() {
                    return Err("GlobalLock failed".to_string());
                }
                std::ptr::copy_nonoverlapping(dib.as_ptr(), ptr as *mut u8, total);
                let _ = GlobalUnlock(hglobal);
                SetClipboardData(CF_DIB, Some(HANDLE(hglobal.0)))
                    .map_err(|e| format!("SetClipboardData failed: {e}"))?;
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        inner
    }
}
