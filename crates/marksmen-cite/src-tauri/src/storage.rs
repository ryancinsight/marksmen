//! Persistent storage for references and collections.

use crate::model::{Collection, Reference};
use std::fs;
use tauri::Manager;

fn db_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("cite_library");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[tauri::command]
pub fn load_references(app: tauri::AppHandle) -> Result<Vec<Reference>, String> {
    let path = db_path(&app)?.join("references.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_references(
    app: tauri::AppHandle,
    references: Vec<Reference>,
) -> Result<(), String> {
    let path = db_path(&app)?.join("references.json");
    let data = serde_json::to_string_pretty(&references).map_err(|e| e.to_string())?;
    fs::write(&path, data).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_collections(app: tauri::AppHandle) -> Result<Vec<Collection>, String> {
    let path = db_path(&app)?.join("collections.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_collections(
    app: tauri::AppHandle,
    collections: Vec<Collection>,
) -> Result<(), String> {
    let path = db_path(&app)?.join("collections.json");
    let data = serde_json::to_string_pretty(&collections).map_err(|e| e.to_string())?;
    fs::write(&path, data).map_err(|e| e.to_string())
}

/// Return the absolute path to the library directory for display/backup use.
#[tauri::command]
pub fn get_library_path(app: tauri::AppHandle) -> Result<String, String> {
    Ok(db_path(&app)?.to_string_lossy().into_owned())
}
