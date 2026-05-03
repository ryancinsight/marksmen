//! marksmen-cite Tauri application entry point.

mod export;
mod fetch;
mod import;
mod model;
mod storage;
mod server;
mod sync;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            tauri::async_runtime::block_on(async {
                server::start_server(app.handle().clone()).await;
            });
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            // Storage
            storage::load_references,
            storage::save_references,
            storage::load_collections,
            storage::save_collections,
            storage::get_library_path,
            storage::merge_cloud_sync,
            // Fetch
            fetch::fetch_doi,
            fetch::fetch_pmid,
            fetch::fetch_arxiv,
            fetch::fetch_isbn,
            // Import
            import::import_pdf,
            import::open_pdf_native,
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
