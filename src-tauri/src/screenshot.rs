use std::path::PathBuf;
use tauri::Manager;

#[derive(serde::Deserialize, Clone, Copy)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// (width, height, RGBA pixels) of the monitor captured for the in-progress snip.
static SNIP_BUFFER: std::sync::Mutex<Option<(i32, i32, Vec<u8>)>> = std::sync::Mutex::new(None);
/// Monitor region the snip covers, remembered so snip_freeze needs no arguments.
static SNIP_REGION: std::sync::Mutex<Option<Region>> = std::sync::Mutex::new(None);

#[tauri::command]
pub async fn take_screenshot(app: tauri::AppHandle, region: Option<Region>) -> Result<String, String> {
    if crate::state::SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("Nectar is shutting down".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || capture_screenshot(&app, region))
        .await
        .map_err(|e| format!("screenshot task failed: {e}"))?
}

/// Starts Nectar's own region-select snip. v2 never touches OS visibility of the
/// notch/dock windows (that desynced smart-mode state and click-through bookkeeping);
/// it only moves the always-on-top overlay over the monitor and dims it. The frozen
/// frame is captured later by snip_freeze, once the command center has auto-hidden.
#[tauri::command]
pub fn start_snip(app: tauri::AppHandle, region: Region) -> Result<(), String> {
    use tauri::Emitter;

    if crate::state::SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("Nectar is shutting down".to_string());
    }
    // Set SNIP_ACTIVE first so the mouse hook freezes all hit-testing immediately.
    if crate::state::SNIP_ACTIVE.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return Err("snip already in progress".to_string());
    }
    if let Ok(mut r) = SNIP_REGION.lock() { *r = Some(region); }

    let shown = if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.set_position(tauri::PhysicalPosition::new(region.x, region.y));
        let _ = win.set_size(tauri::PhysicalSize::new(region.width as u32, region.height as u32));
        let _ = win.set_ignore_cursor_events(false);
        let _ = win.show();
        // Sit above notch/dock in z-order for the duration of the snip.
        if let Ok(hwnd) = win.hwnd() {
            let hwnd_val = hwnd.0 as isize;
            crate::utils::re_assert_topmost(windows::Win32::Foundation::HWND(hwnd_val as *mut _));
        }
        true
    } else {
        false
    };
    if !shown {
        cleanup_snip(&app);
        return Err("overlay window not available".to_string());
    }
    // Plain dim first — the frozen frame follows via "snip-frame".
    if let Err(e) = app.emit_to("overlay", "snip-begin", serde_json::json!({ "image": null })) {
        cleanup_snip(&app);
        return Err(format!("failed to emit snip-begin: {e}"));
    }
    Ok(())
}

/// Second phase, fired by the overlay ~700ms into the snip: by then the cursor has left
/// the notch and the command center has auto-hidden, so the captured frame is clean.
#[tauri::command]
pub async fn snip_freeze(app: tauri::AppHandle) -> Result<(), String> {
    use base64::Engine;
    use tauri::Emitter;

    let region = SNIP_REGION.lock().ok().and_then(|r| *r);
    let Some(region) = region else {
        return Err("no snip in progress".to_string());
    };

    let result = tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(600));
        if !crate::state::SNIP_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("snip cancelled before freeze".to_string());
        }
        let captured = unsafe { capture_screen_region(region.x, region.y, region.width, region.height) };
        captured.and_then(|(width, height, pixels)| {
            let png = encode_png(width, height, &pixels)?;
            if let Ok(mut b) = SNIP_BUFFER.lock() { *b = Some((width, height, pixels)); }
            Ok(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            ))
        })
    })
    .await
    .map_err(|e| format!("snip freeze task failed: {e}"))
    .and_then(|r| r);

    match result {
        Ok(image) => {
            if let Err(e) = app.emit_to("overlay", "snip-frame", serde_json::json!({ "image": image })) {
                cleanup_snip(&app);
                return Err(format!("failed to emit snip-frame: {e}"));
            }
            Ok(())
        }
        Err(e) => {
            let _ = app.emit_to("overlay", "snip-abort", ());
            cleanup_snip(&app);
            Err(e)
        }
    }
}

/// Crops the selected region out of the captured frame, saves the PNG and puts it on the
/// clipboard. x/y/width/height are logical CSS px relative to the overlay window.
#[tauri::command]
pub async fn finish_snip(app: tauri::AppHandle, x: i32, y: i32, width: i32, height: i32, scale: f64) -> Result<String, String> {
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let buffer = SNIP_BUFFER.lock().ok().and_then(|mut b| b.take());
        // Freeze never completed (fast cancel, capture failure) — just clean up.
        let Some((buf_w, buf_h, pixels)) = buffer else {
            return Ok(String::new());
        };

        // Logical (CSS px) -> physical pixels, clamped into the captured buffer.
        let px = (x as f64 * scale).round() as i32;
        let py = (y as f64 * scale).round() as i32;
        let mut pw = (width as f64 * scale).round() as i32;
        let mut ph = (height as f64 * scale).round() as i32;
        let px = px.clamp(0, buf_w);
        let py = py.clamp(0, buf_h);
        pw = pw.min(buf_w - px);
        ph = ph.min(buf_h - py);

        // Too small to be meaningful — treat as a cancel, just clean up.
        if pw < 2 || ph < 2 {
            return Ok(String::new());
        }

        // Crop rows out of the top-down RGBA buffer.
        let stride = buf_w as usize * 4;
        let row_bytes = pw as usize * 4;
        let mut cropped = vec![0u8; row_bytes * ph as usize];
        for row in 0..ph as usize {
            let src_start = (py as usize + row) * stride + px as usize * 4;
            cropped[row * row_bytes..(row + 1) * row_bytes]
                .copy_from_slice(&pixels[src_start..src_start + row_bytes]);
        }

        let png = encode_png(pw, ph, &cropped)?;
        let dir = screenshots_dir().ok_or_else(|| "Pictures folder not found".to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
        let filename = chrono::Local::now()
            .format("Nectar Screenshot %Y%m%d-%H%M%S.png")
            .to_string();
        let path = dir.join(filename);
        std::fs::write(&path, &png).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        if let Err(e) = set_clipboard_dib(None, pw, ph, &cropped) {
            eprintln!("snip: clipboard set failed: {e}");
        }
        Ok(path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("snip finish task failed: {e}"));

    cleanup_snip(&app);
    result?
}

#[tauri::command]
pub fn cancel_snip(app: tauri::AppHandle) -> Result<(), String> {
    cleanup_snip(&app);
    Ok(())
}

/// Hides the overlay again, re-enables click-through, and clears snip state.
/// Notch/dock were never hidden, so there is nothing to restore. Safe to call
/// when no snip is in progress (statics are empty then).
fn cleanup_snip(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.hide();
        let _ = win.set_ignore_cursor_events(true);
    }
    crate::state::SNIP_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut b) = SNIP_BUFFER.lock() { *b = None; }
    if let Ok(mut r) = SNIP_REGION.lock() { *r = None; }
}

fn capture_screenshot(app: &tauri::AppHandle, region: Option<Region>) -> Result<String, String> {
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

    let capture = match &region {
        Some(r) => (r.x, r.y, r.width, r.height),
        None => {
            let (vx, vy, vw, vh) = virtual_screen_rect()?;
            (vx, vy, vw, vh)
        }
    };
    let result = unsafe { capture_screen_region(capture.0, capture.1, capture.2, capture.3) };

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

fn virtual_screen_rect() -> Result<(i32, i32, i32, i32), String> {
    unsafe {
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
        Ok((vx, vy, width, height))
    }
}

/// Captures a rectangle of the virtual screen as top-down RGBA pixels.
/// Origin can be negative (monitors left of/above the primary) — BitBlt handles it.
unsafe fn capture_screen_region(x: i32, y: i32, width: i32, height: i32) -> Result<(i32, i32, Vec<u8>), String> {
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
        DIB_RGB_COLORS, SRCCOPY,
    };

    if width <= 0 || height <= 0 {
        return Err("capture region has no area".to_string());
    }

    let hdc_screen = GetDC(None);
    let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
    let hbm_mem = CreateCompatibleBitmap(hdc_screen, width, height);
    let h_old = SelectObject(hdc_mem, hbm_mem.into());

    let mut result = Err("BitBlt failed".to_string());
    if BitBlt(hdc_mem, 0, 0, width, height, Some(hdc_screen), x, y, SRCCOPY | CAPTUREBLT).is_ok() {
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
