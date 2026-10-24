use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Serialize, Deserialize, Clone)]
pub struct Note {
    pub id: String,
    pub text: String,
    pub updated_at: i64,
}

fn notes_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join("notes.json"))
}

fn read_all(app: &tauri::AppHandle) -> Vec<Note> {
    let Ok(path) = notes_path(app) else { return Vec::new() };
    let Ok(content) = std::fs::read_to_string(path) else { return Vec::new() };
    serde_json::from_str(&content).unwrap_or_default()
}

fn write_all(app: &tauri::AppHandle, notes: &[Note]) -> Result<(), String> {
    let path = notes_path(app)?;
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let content = serde_json::to_string_pretty(notes).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn notes_list(app: tauri::AppHandle) -> Vec<Note> {
    let mut notes = read_all(&app);
    notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    notes
}

#[tauri::command]
pub fn notes_save(app: tauri::AppHandle, note: Note) -> Result<(), String> {
    let mut notes = read_all(&app);
    if let Some(existing) = notes.iter_mut().find(|n| n.id == note.id) {
        *existing = note;
    } else {
        notes.push(note);
    }
    write_all(&app, &notes)
}

#[tauri::command]
pub fn notes_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let notes: Vec<Note> = read_all(&app).into_iter().filter(|n| n.id != id).collect();
    write_all(&app, &notes)
}
