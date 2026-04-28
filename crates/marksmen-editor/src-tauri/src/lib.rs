use marksmen_core::config::Config;
use marksmen_core::parsing::parser;
use marksmen_docx::translation::document::convert as convert_docx;
use marksmen_html::convert as convert_html;
use marksmen_odt::translate_and_render as convert_odt;
use marksmen_ppt::convert as convert_ppt;

// ── HTML ↔ Markdown sync ────────────────────────────────────────────────────

#[tauri::command]
fn html_to_md(html: String) -> Result<String, String> {
    marksmen_html_read::parse_html(&html).map_err(|e| e.to_string())
}

#[tauri::command]
fn md_to_html(markdown: String) -> Result<String, String> {
    use marksmen_core::config::frontmatter::parse_frontmatter;
    // Strip YAML frontmatter so it is never rendered as visible text in the editor.
    // parse_frontmatter(&str) -> Result<(&str, FrontMatterConfig)>
    let (body, fm) = parse_frontmatter(&markdown).unwrap_or((&markdown, Default::default()));
    let config = Config::default().merge_frontmatter(&fm);
    let events = parser::parse(body);
    convert_html(events, &config).map_err(|e| e.to_string())
}

// ── File Import via native OS dialog ───────────────────────────────────────

#[tauri::command]
async fn import_file(app: tauri::AppHandle) -> Result<(String, String), String> {
    use tauri_plugin_dialog::DialogExt;

    let file_path = app
        .dialog()
        .file()
        .add_filter("Documents", &["md", "html", "docx", "odt", "pdf", "typ"])
        .blocking_pick_file()
        .ok_or_else(|| "No file selected".to_string())?;

    let path = file_path.into_path().map_err(|e| e.to_string())?;
    let filename = path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
        
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    let content = match ext.as_str() {
        "md" => std::fs::read_to_string(&path).map_err(|e| e.to_string()),
        "html" => {
            let html = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            marksmen_html_read::parse_html(&html).map_err(|e| e.to_string())
        }
        "docx" => {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            marksmen_docx_read::parse_docx(&bytes, None).map_err(|e| e.to_string())
        }
        "odt" => {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            marksmen_odt_read::parse_odt(&bytes, None).map_err(|e| e.to_string())
        }
        "pdf" => {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            marksmen_pdf_read::parse_pdf(&bytes).map_err(|e| e.to_string())
        }
        "typ" => {
            let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            marksmen_typst_read::parse_typst(&content).map_err(|e| e.to_string())
        }
        other => Err(format!("Unsupported extension: {other}")),
    }?;
    
    Ok((content, filename))
}

// ── Tracked Changes via marksmen-diff ─────────────────────────────────────

#[tauri::command]
fn generate_diff(old_md: String, new_md: String) -> String {
    // diff_markdown returns a String with HTML representing the diffed AST
    marksmen_diff::diff_markdown(&old_md, &new_md)
}

// ── Export ─────────────────────────────────────────────────────────────────

#[tauri::command]
fn export_format(
    markdown: String,
    format: String,
    doc_name: Option<String>,
) -> Result<(String, String, String), String> {
    use base64::Engine as _;
    let enc = base64::engine::general_purpose::STANDARD;

    // Derive a safe filesystem stem from the document title.
    // Strip any existing extension, then sanitize path-unsafe characters.
    let stem: String = doc_name
        .as_deref()
        .map(|n| {
            let s = if let Some(idx) = n.rfind('.') { &n[..idx] } else { n };
            s.chars()
                .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
                .collect()
        })
        .unwrap_or_else(|| "document".to_string());

    let config = Config::default();
    let events = parser::parse(&markdown);

    match format.as_str() {
        "docx" => {
            let bytes = convert_docx(events, &config, std::path::Path::new("."), None)
                .map_err(|e| e.to_string())?;
            Ok((
                enc.encode(&bytes),
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into(),
                format!("{stem}.docx"),
            ))
        }
        "odt" => {
            let bytes = convert_odt(&events, &config, std::path::Path::new("."))
                .map_err(|e| e.to_string())?;
            Ok((
                enc.encode(&bytes),
                "application/vnd.oasis.opendocument.text".into(),
                format!("{stem}.odt"),
            ))
        }
        "pdf" => {
            let bytes =
                marksmen_pdf::convert(&markdown, &config, None).map_err(|e| e.to_string())?;
            Ok((
                enc.encode(&bytes),
                "application/pdf".into(),
                format!("{stem}.pdf"),
            ))
        }
        "ppt" => {
            let bytes = convert_ppt(events, &config).map_err(|e| e.to_string())?;
            Ok((
                enc.encode(&bytes),
                "application/vnd.openxmlformats-officedocument.presentationml.presentation".into(),
                format!("{stem}.pptx"),
            ))
        }
        "markdown" => Ok((
            enc.encode(markdown.as_bytes()),
            "text/markdown".into(),
            format!("{stem}.md"),
        )),
        "html" => {
            let html = convert_html(events, &config).map_err(|e| e.to_string())?;
            Ok((
                enc.encode(html.as_bytes()),
                "text/html".into(),
                format!("{stem}.html"),
            ))
        }
        "typst" => {
            let typst = marksmen_typst::translator::translate(events, &config)
                .map_err(|e| e.to_string())?;
            Ok((
                enc.encode(typst.as_bytes()),
                "text/plain".into(),
                format!("{stem}.typ"),
            ))
        }
        _ => Err(format!("Unknown format: {format}")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            html_to_md,
            md_to_html,
            export_format,
            import_file,
            generate_diff
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
