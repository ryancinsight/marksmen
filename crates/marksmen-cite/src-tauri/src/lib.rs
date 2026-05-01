//! marksmen-cite Tauri application entry point.

mod export;
mod fetch;
mod import;
mod model;
mod storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            // Storage
            storage::load_references,
            storage::save_references,
            storage::load_collections,
            storage::save_collections,
            storage::get_library_path,
            // Fetch
            fetch::fetch_doi,
            fetch::fetch_pmid,
            fetch::fetch_arxiv,
            fetch::fetch_isbn,
            // Import
            import::import_pdf,
            import::import_ris,
            import::import_bibtex,
            import::import_lib_file,
            // Export
            export::export_ris,
            export::export_bibtex,
            export::format_citation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
