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
pub fn save_references(app: tauri::AppHandle, references: Vec<Reference>) -> Result<(), String> {
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
pub fn save_collections(app: tauri::AppHandle, collections: Vec<Collection>) -> Result<(), String> {
    let path = db_path(&app)?.join("collections.json");
    let data = serde_json::to_string_pretty(&collections).map_err(|e| e.to_string())?;
    fs::write(&path, data).map_err(|e| e.to_string())
}

/// Return the absolute path to the library directory for display/backup use.
#[tauri::command]
pub fn get_library_path(app: tauri::AppHandle) -> Result<String, String> {
    Ok(db_path(&app)?.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn merge_cloud_sync(
    app: tauri::AppHandle,
    remote_payload: crate::sync::SyncPayload,
    device_id: String,
) -> Result<crate::sync::SyncPayload, String> {
    let local_refs = load_references(app.clone())?;
    let local_cols = load_collections(app.clone())?;

    let local_payload = crate::sync::SyncPayload::new(device_id, local_refs, local_cols);
    let merged = crate::sync::merge_sync_payloads(local_payload, remote_payload);

    // Save merged state back to local disk
    save_references(app.clone(), merged.references.clone())?;
    save_collections(app.clone(), merged.collections.clone())?;

    Ok(merged)
}
