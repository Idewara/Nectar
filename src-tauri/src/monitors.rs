use tauri::{AppHandle, Emitter, Manager, Monitor};
use std::collections::HashMap;
use std::sync::atomic::Ordering;

use crate::state::*;
use crate::types::{MonitorMode, MonitorInfo, WindowKind};
use crate::utils::get_setting_str;

pub fn monitor_stable_id(m: &Monitor, index: usize) -> String {
    m.name().cloned().unwrap_or_else(|| format!("idx-{index}"))
}

fn sanitize_for_label(raw: &str) -> String {
    raw.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

fn same_monitor(a: &Monitor, b: &Monitor) -> bool {
    *a.position() == *b.position() && *a.size() == *b.size()
}

fn find_primary_index(app: &AppHandle, monitors: &[Monitor]) -> usize {
    if let Ok(Some(primary)) = app.primary_monitor() {
        if let Some(i) = monitors.iter().position(|m| same_monitor(m, &primary)) {
            return i;
        }
    }
    0
}

fn mode_and_id_keys(kind: WindowKind) -> (&'static str, &'static str) {
    match kind {
        WindowKind::Dock => ("nectar-dock-monitor-mode", "nectar-dock-monitor-id"),
        WindowKind::Notch => ("nectar-notch-monitor-mode", "nectar-notch-monitor-id"),
    }
}

pub fn resolve_target_monitor(app: &AppHandle, kind: WindowKind) -> Option<Monitor> {
    let (mode_key, id_key) = mode_and_id_keys(kind);
    let mode = MonitorMode::from_setting(get_setting_str(app, mode_key).as_deref());

    if mode == MonitorMode::Specific {
        if let Some(chosen_id) = get_setting_str(app, id_key) {
            if let Ok(monitors) = app.available_monitors() {
                if let Some(m) = monitors.iter().enumerate()
                    .find(|(i, m)| monitor_stable_id(m, *i) == chosen_id)
                    .map(|(_, m)| m.clone())
                {
                    return Some(m);
                }
            }
        }
    }

    app.primary_monitor().ok().flatten()
}

pub fn monitor_for_window_label(app: &AppHandle, kind: WindowKind, label: &str) -> Option<Monitor> {
    match monitor_suffix(label) {
        Some(sanitized_id) => {
            let monitors = app.available_monitors().ok()?;
            monitors.iter().enumerate()
                .find(|(i, m)| sanitize_for_label(&monitor_stable_id(m, *i)) == sanitized_id)
                .map(|(_, m)| m.clone())
        }
        None => resolve_target_monitor(app, kind),
    }
}

fn mode_setting_keys(kind: WindowKind) -> (&'static str, &'static str) {
    match kind {
        WindowKind::Dock => ("nectar-dock-mode", "nectar-dock-mode-by-monitor"),
        WindowKind::Notch => ("nectar-notch-mode", "nectar-notch-mode-by-monitor"),
    }
}

/// Resolves which mode (fixed/smart/peek) a specific dock or notch window
/// should use. Per-monitor overrides live in a JSON map keyed by the same
/// stable monitor id used elsewhere (`nectar-dock-mode-by-monitor` etc.);
/// a monitor without an entry falls back to the plain global mode setting.
pub fn resolve_mode_for_label(app: &AppHandle, kind: WindowKind, label: &str) -> String {
    let (default_key, map_key) = mode_setting_keys(kind);
    let normalize = |m: String| if m == "auto-hide" { "smart".to_string() } else { m };
    // Fresh installs: smart dock, fixed notch.
    let fallback = if matches!(kind, WindowKind::Dock) { "smart" } else { "fixed" };
    let default_mode = normalize(get_setting_str(app, default_key).unwrap_or_else(|| fallback.to_string()));

    let Some(monitor) = monitor_for_window_label(app, kind, label) else { return default_mode };
    let Some(monitor_id) = monitor.name().cloned() else { return default_mode };
    let Some(map_json) = get_setting_str(app, map_key) else { return default_mode };
    let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&map_json) else { return default_mode };
    map.get(&monitor_id).cloned().map(normalize).unwrap_or(default_mode)
}

/// Sets the mode override for whichever monitor a window is currently on,
/// so an in-notch/in-dock mode switcher affects only that screen.
pub fn set_mode_for_label(app: &AppHandle, kind: WindowKind, label: &str, mode: &str) -> Result<(), String> {
    let (_, map_key) = mode_setting_keys(kind);
    let monitor = monitor_for_window_label(app, kind, label).ok_or("no monitor for this window")?;
    let monitor_id = monitor.name().cloned().ok_or("monitor has no stable id")?;

    let mut map: HashMap<String, String> = get_setting_str(app, map_key)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    map.insert(monitor_id, mode.to_string());
    let json = serde_json::to_string(&map).map_err(|e| e.to_string())?;
    crate::commands::save_setting(app.clone(), map_key.to_string(), serde_json::Value::String(json))
}

pub fn list_monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    let monitors = app.available_monitors().unwrap_or_default();
    let primary_index = find_primary_index(app, &monitors);
    monitors.iter().enumerate().map(|(i, m)| {
        let pos = m.position();
        let size = m.size();
        MonitorInfo {
            id: monitor_stable_id(m, i),
            label: format!("Monitor {} ({}\u{d7}{})", i + 1, size.width, size.height),
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
            is_primary: i == primary_index,
        }
    }).collect()
}

fn desired_extra_windows(app: &AppHandle, kind: WindowKind, monitors: &[Monitor]) -> HashMap<String, Monitor> {
    let mut desired = HashMap::new();
    let (mode_key, _) = mode_and_id_keys(kind);
    let mode = MonitorMode::from_setting(get_setting_str(app, mode_key).as_deref());
    if mode != MonitorMode::All {
        return desired;
    }

    let primary_index = find_primary_index(app, monitors);
    for (i, m) in monitors.iter().enumerate() {
        if i == primary_index {
            continue;
        }
        let id = sanitize_for_label(&monitor_stable_id(m, i));
        let label = match kind {
            WindowKind::Dock => dock_label_for_monitor(&id),
            WindowKind::Notch => notch_label_for_monitor(&id),
        };
        desired.insert(label, m.clone());
    }
    desired
}

fn destroy_monitor_window(app: &AppHandle, label: &str) {
    if let Some(win) = app.get_webview_window(label) {
        if let Ok(hwnd) = win.hwnd() {
            crate::services::unregister_appbar_native(hwnd);
        }
        let _ = win.destroy();
    }
    clear_window_state(label);
}

fn sync_kind(app: &AppHandle, kind: WindowKind, monitors: &[Monitor]) {
    let desired = if kind == WindowKind::Dock {
        let dock_enabled = get_setting_str(app, "nectar-dock-enabled").map(|v| v == "true").unwrap_or(true);
        if !dock_enabled { HashMap::new() } else { desired_extra_windows(app, kind, monitors) }
    } else {
        desired_extra_windows(app, kind, monitors)
    };

    let is_extra_label_of_kind = |label: &str| -> bool {
        monitor_suffix(label).is_some() && match kind {
            WindowKind::Dock => is_dock_label(label),
            WindowKind::Notch => is_notch_label(label),
        }
    };

    let existing: Vec<String> = app.webview_windows().keys()
        .filter(|l| is_extra_label_of_kind(l))
        .cloned()
        .collect();

    for label in &existing {
        if !desired.contains_key(label) {
            destroy_monitor_window(app, label);
        }
    }

    for (label, monitor) in &desired {
        if app.get_webview_window(label).is_none() {
            crate::services::create_monitor_window(app, kind, label, monitor);
        }
    }
}

pub fn sync_monitor_windows(app: &AppHandle) {
    if MONITOR_SYNC_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return;
    }

    let monitors = app.available_monitors().unwrap_or_default();
    if !monitors.is_empty() {
        sync_kind(app, WindowKind::Dock, &monitors);
        sync_kind(app, WindowKind::Notch, &monitors);
        let _ = app.emit("monitors-changed", ());
    }

    MONITOR_SYNC_IN_PROGRESS.store(false, Ordering::SeqCst);
}

/// Frontend crossed the drag threshold on the notch's empty area — the global mouse
/// hook takes over cursor tracking from here (the webview loses the cursor off-window).
#[tauri::command]
pub fn begin_monitor_drag() {
    MONITOR_DRAG_ACTIVE.store(true, Ordering::Relaxed);
    if let Ok(mut d) = MONITOR_DRAG_DWELL.lock() { *d = None; }
}

/// Stable id of the monitor containing the point, if it differs from the notch's
/// current target monitor. Ids come from the same `monitor_stable_id` path
/// `resolve_target_monitor` uses, so the comparison is apples-to-apples.
pub fn monitor_drag_candidate(app: &AppHandle, x: i32, y: i32) -> Option<String> {
    let monitors = app.available_monitors().ok()?;
    let (hovered_idx, hovered) = monitors.iter().enumerate().find(|(_, m)| {
        let p = m.position();
        let s = m.size();
        x >= p.x && x < p.x + s.width as i32 && y >= p.y && y < p.y + s.height as i32
    })?;
    let hovered_id = monitor_stable_id(hovered, hovered_idx);
    let current = resolve_target_monitor(app, WindowKind::Notch)?;
    let current_id = monitors.iter().enumerate()
        .find(|(_, m)| same_monitor(m, &current))
        .map(|(i, m)| monitor_stable_id(m, i))?;
    if hovered_id != current_id { Some(hovered_id) } else { None }
}

/// Drag-to-move retarget: point BOTH dock and notch at `device_name` in a single
/// settings write, then run the same re-sync side effects `save_setting` runs for
/// monitor-targeting keys (one resync, not one per key).
pub fn retarget_windows_to_monitor(app: &AppHandle, device_name: &str) -> Result<(), String> {
    let path = app.path().app_config_dir().map_err(|e| e.to_string())?.join("settings.json");
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let mut settings = HashMap::new();
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(existing) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&content) {
            settings = existing;
        }
    }
    let specific = serde_json::Value::String("specific".to_string());
    let id = serde_json::Value::String(device_name.to_string());
    settings.insert("nectar-dock-monitor-mode".to_string(), specific.clone());
    settings.insert("nectar-dock-monitor-id".to_string(), id.clone());
    settings.insert("nectar-notch-monitor-mode".to_string(), specific);
    settings.insert("nectar-notch-monitor-id".to_string(), id);
    let content = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())?;

    crate::utils::replace_settings_cache(settings.clone());

    for key in [
        "nectar-dock-monitor-mode",
        "nectar-dock-monitor-id",
        "nectar-notch-monitor-mode",
        "nectar-notch-monitor-id",
    ] {
        let _ = app.emit("settings-changed", serde_json::json!({ "key": key, "value": &settings[key] }));
    }

    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        sync_monitor_windows(&app_clone);
        let dock_enabled = crate::utils::get_setting_str(&app_clone, "nectar-dock-enabled")
            .map(|v| v == "true").unwrap_or(true);
        if dock_enabled {
            let dock_mode = crate::utils::get_setting_str(&app_clone, "nectar-dock-mode")
                .unwrap_or_else(|| "smart".to_string());
            crate::commands::change_dock_mode(app_clone.clone(), dock_mode).await;
        }
        let notch_mode = crate::utils::get_setting_str(&app_clone, "nectar-notch-mode")
            .unwrap_or_else(|| "fixed".to_string());
        crate::commands::change_notch_mode(app_clone.clone(), notch_mode).await;
    });

    Ok(())
}
