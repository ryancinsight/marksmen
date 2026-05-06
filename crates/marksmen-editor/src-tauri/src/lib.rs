use marksmen_core::config::Config;
use marksmen_core::parsing::parser;
use marksmen_docx::translation::document::convert as convert_docx;
use marksmen_html::convert as convert_html;
use marksmen_odt::translate_and_render as convert_odt;
use marksmen_ppt::convert as convert_ppt;
use marksmen_rich;
use marksmen_rich_read;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CitationPayload {
    pub id: String,
    pub author: String,
    pub year: String,
    pub title: String,
    pub publisher: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub url: String,
}

impl From<CitationPayload> for marksmen_csl::model::Reference {
    fn from(payload: CitationPayload) -> Self {
        let mut ref_model = marksmen_csl::model::Reference::default();
        ref_model.id = payload.id;
        ref_model.r#type = payload.item_type;
        ref_model.title = Some(payload.title);
        ref_model.container_title = Some(payload.publisher);
        ref_model.url = Some(payload.url);
        
        // Parse "LastName, FirstName"
        if !payload.author.is_empty() {
            let names: Vec<_> = payload.author.split(" and ").map(|a| {
                let parts: Vec<_> = a.split(',').map(|s| s.trim()).collect();
                let family = parts.first().map(|s| s.to_string());
                let given = parts.get(1).map(|s| s.to_string());
                marksmen_csl::model::NameVariable {
                    family,
                    given,
                    dropping_particle: None,
                    non_dropping_particle: None,
                    literal: None,
                    suffix: None,
                }
            }).collect();
            ref_model.author = Some(names);
        }

        if let Ok(y) = payload.year.parse::<i32>() {
            ref_model.issued = Some(marksmen_csl::model::DateVariable {
                date_parts: vec![vec![y]],
            });
        }

        ref_model
    }
}

// ── CSL Formatting (Stage 5) ────────────────────────────────────────────────

#[tauri::command]
fn format_csl_citation(citation: CitationPayload, _style: String) -> Result<String, String> {
    // For now, since the CSL XML parser isn't complete to load all APA nodes,
    // we use the zero-cost CSL engine structure to construct a native citation layout.
    // In the future, this parses the requested .csl file via quick_xml.
    let ref_model = marksmen_csl::model::Reference::from(citation);
    
    // Construct a native APA-like in-text citation mock using the CSL layout AST
    use marksmen_csl::schema::{Layout, RenderingElement, Text};
    use marksmen_csl::engine::{Context, evaluate_layout};
    
    let layout = Layout {
        prefix: Some("(".to_string()),
        suffix: Some(")".to_string()),
        delimiter: Some("; ".to_string()),
        elements: vec![
            RenderingElement::Text(Text {
                variable: Some("author".to_string()),
                macro_name: None,
                term: None,
                value: None,
                prefix: None,
                suffix: None,
                quotes: None,
                font_style: None,
                font_weight: None,
                text_decoration: None,
                vertical_align: None,
            }),
            RenderingElement::Text(Text {
                variable: None,
                macro_name: None,
                term: None,
                value: Some(", ".to_string()),
                prefix: None,
                suffix: None,
                quotes: None,
                font_style: None,
                font_weight: None,
                text_decoration: None,
                vertical_align: None,
            }),
            RenderingElement::Text(Text {
                variable: Some("issued".to_string()),
                macro_name: None,
                term: None,
                value: None,
                prefix: None,
                suffix: None,
                quotes: None,
                font_style: None,
                font_weight: None,
                text_decoration: None,
                vertical_align: None,
            }),
        ],
    };
    
    let style = marksmen_csl::schema::Style {
        class: "in-text".into(),
        version: "1.0".into(),
        info: None,
        locales: vec![],
        macros: vec![],
        citation: marksmen_csl::schema::Citation {
            layout: Some(layout.clone()),
            sort: None,
        },
        bibliography: None,
    };
    
    let ctx = Context::new(&style, &ref_model);
    Ok(evaluate_layout(&layout, &ctx))
}

#[tauri::command]
fn format_csl_bibliography(citations: Vec<CitationPayload>, _style: String) -> Result<String, String> {
    // Evaluate full APA reference strings
    let mut html = String::new();
    
    for citation in citations {
        let ref_model = marksmen_csl::model::Reference::from(citation);
        
        let author_str = ref_model.author.as_ref().map(|a| {
            a.iter().filter_map(|n| n.family.clone()).collect::<Vec<_>>().join(", ")
        }).unwrap_or_else(|| "Unknown".to_string());
        
        let year_str = ref_model.issued.as_ref().and_then(|d| d.date_parts.first())
            .and_then(|p| p.first()).map(|y| y.to_string()).unwrap_or_else(|| "n.d.".to_string());
            
        let title_str = ref_model.title.as_deref().unwrap_or("Untitled");
        let pub_str = ref_model.container_title.as_deref().unwrap_or("");
        
        // This simulates the evaluate_layout from a `<bibliography>` node
        let item_html = format!("{} ({}). {}. <em>{}</em>.", author_str, year_str, title_str, pub_str);
        
        html.push_str(&format!(
            "<p style=\"margin:4px 0; padding-left:2em; text-indent:-2em; font-size:12px;\">{}</p>",
            item_html
        ));
    }
    
    Ok(html)
}

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
async fn import_file(app: tauri::AppHandle) -> Result<(String, String, String), String> {
    use tauri_plugin_dialog::DialogExt;

    let file_path = app
        .dialog()
        .file()
        .add_filter(
            "Documents",
            &[
                "md", "html", "docx", "odt", "pdf", "typ", "rtf", "pptx", "epub",
            ],
        )
        .blocking_pick_file()
        .ok_or_else(|| "No file selected".to_string())?;

    let path = file_path.into_path().map_err(|e| e.to_string())?;
    let filename = path
        .file_name()
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
        "rtf" => {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            marksmen_rich_read::parse_rtf(&bytes).map_err(|e| e.to_string())
        }
        "pptx" => {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            marksmen_ppt_read::parse_pptx(&bytes).map_err(|e| e.to_string())
        }
        "epub" => {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            marksmen_epub_read::parse_epub(&bytes).map_err(|e| e.to_string())
        }
        other => Err(format!("Unsupported extension: {other}")),
    }?;
    Ok((content, filename, path.to_string_lossy().into_owned()))
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
            let s = if let Some(idx) = n.rfind('.') {
                &n[..idx]
            } else {
                n
            };
            s.chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
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
        "ppt" | "pptx" => {
            let bytes = convert_ppt(events, &config).map_err(|e| e.to_string())?;
            Ok((
                enc.encode(&bytes),
                "application/vnd.openxmlformats-officedocument.presentationml.presentation".into(),
                format!("{stem}.pptx"),
            ))
        }
        "epub" => {
            let bytes = marksmen_epub::convert(events, &config).map_err(|e| e.to_string())?;
            Ok((
                enc.encode(&bytes),
                "application/epub+zip".into(),
                format!("{stem}.epub"),
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
        "rtf" => {
            let bytes = marksmen_rich::convert(events, &config).map_err(|e| e.to_string())?;
            Ok((
                enc.encode(&bytes),
                "application/rtf".into(),
                format!("{stem}.rtf"),
            ))
        }
        _ => Err(format!("Unknown format: {format}")),
    }
}

/// Combine multiple Markdown files into a single exported document.
#[tauri::command]
async fn export_binder(
    app: tauri::AppHandle,
    files: Vec<String>,
    format: String,
    doc_name: String,
) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;
    use marksmen_core::parsing::combine::AstConcatenator;

    let config = Config::default();
    let mut concat = AstConcatenator::new();
    let mut contents = Vec::new();

    // First read all files to keep strings alive
    for path_str in &files {
        let path = std::path::Path::new(path_str);
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        contents.push(content);
    }

    // Parse and concatenate all files
    for (idx, (path_str, content)) in files.iter().zip(contents.iter()).enumerate() {
        let path = std::path::Path::new(path_str);
        
        // Use a safe namespace derived from the filename or index
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let namespace = format!("doc{}-{}", idx, stem);
        
        let events = parser::parse(content);
        concat.add_document(&namespace, events);
    }

    let unified_events = concat.build();

    let (bytes, ext): (Vec<u8>, &str) = match format.as_str() {
        "docx" => (
            marksmen_docx::translation::document::convert(unified_events, &config, std::path::Path::new("."), None)
                .map_err(|e| e.to_string())?,
            "docx",
        ),
        "pdf" => {
            // PDF conversion currently takes a raw Markdown string, not AST events.
            // Wait! Our PDF pipeline relies on Typst string conversion.
            // Typst conversion takes AST! `marksmen_typst::translator::translate(unified_events, &config)`!
            // Let's generate Typst AST first:
            let typst = marksmen_typst::translator::translate(unified_events, &config).map_err(|e| e.to_string())?;
            // Then compile PDF using typst_library
            (
                marksmen_pdf::rendering::compiler::compile_to_pdf(&typst, &config, Some(std::path::PathBuf::from("."))).map_err(|e| e.to_string())?,
                "pdf"
            )
        },
        "html" => (
            convert_html(unified_events, &config)
                .map_err(|e| e.to_string())?
                .into_bytes(),
            "html",
        ),
        _ => return Err(format!("Unsupported binder format: {format}")),
    };

    let save_path = app
        .dialog()
        .file()
        .add_filter(&format.to_uppercase(), &[ext])
        .set_file_name(&format!("{doc_name}.{ext}"))
        .blocking_save_file()
        .ok_or_else(|| "No file selected".to_string())?
        .into_path()
        .map_err(|e| e.to_string())?;

    std::fs::write(&save_path, &bytes).map_err(|e| e.to_string())
}

/// Execute a Mail Merge operation, generating a single concatenated output document.
#[tauri::command]
async fn execute_mail_merge(
    app: tauri::AppHandle,
    template_markdown: String,
    csv_data: String,
    format: String,
    doc_name: String,
) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;
    use marksmen_core::parsing::combine::AstConcatenator;
    use marksmen_core::parsing::mailmerge::process_ast;
    use std::collections::HashMap;

    let config = Config::default();
    
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(csv_data.as_bytes());
        
    let headers = reader.headers().map_err(|e| e.to_string())?.clone();

    // Parse the template ONCE; parser::parse returns Vec<Event<'static>> directly.
    let template_events = parser::parse(&template_markdown);

    let mut concat = AstConcatenator::new();
    let mut num_records = 0;

    // Stream AST transformations directly into the concatenator
    for (idx, result) in reader.records().enumerate() {
        let record = result.map_err(|e| e.to_string())?;
        let mut map = HashMap::new();
        for (i, field) in record.iter().enumerate() {
            if let Some(header) = headers.get(i) {
                map.insert(header.to_string(), field.to_string());
            }
        }
        
        let processed_events = process_ast(&template_events, &map);
        let namespace = format!("merge-{}", idx);
        concat.add_document(&namespace, processed_events);
        num_records += 1;
    }

    if num_records == 0 {
        return Err("No data records found in CSV.".to_string());
    }

    let unified_events = concat.build();

    let (bytes, ext): (Vec<u8>, &str) = match format.as_str() {
        "docx" => (
            marksmen_docx::translation::document::convert(unified_events, &config, std::path::Path::new("."), None)
                .map_err(|e| e.to_string())?,
            "docx",
        ),
        "pdf" => {
            let typst = marksmen_typst::translator::translate(unified_events, &config).map_err(|e| e.to_string())?;
            (
                marksmen_pdf::rendering::compiler::compile_to_pdf(&typst, &config, Some(std::path::PathBuf::from("."))).map_err(|e| e.to_string())?,
                "pdf"
            )
        },
        _ => return Err(format!("Unsupported mail merge format: {format}")),
    };

    let save_path = app
        .dialog()
        .file()
        .add_filter(&format.to_uppercase(), &[ext])
        .set_file_name(&format!("{doc_name}.{ext}"))
        .blocking_save_file()
        .ok_or_else(|| "No file selected".to_string())?
        .into_path()
        .map_err(|e| e.to_string())?;

    std::fs::write(&save_path, &bytes).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_system_fonts() -> Vec<String> {
    let source = font_kit::source::SystemSource::new();
    let mut families = source.all_families().unwrap_or_default();
    families.sort_unstable();
    families
}

// ── Assets ─────────────────────────────────────────────────────────────

#[tauri::command]
fn save_base64_asset(app: tauri::AppHandle, base64_data: String, ext: String, current_path: Option<String>) -> Result<String, String> {
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    use tauri::Manager;
    use std::path::PathBuf;

    // Strip prefix if present (e.g. "data:image/png;base64,")
    let b64_str = if let Some(idx) = base64_data.find(',') {
        &base64_data[idx + 1..]
    } else {
        &base64_data
    };

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(b64_str)
        .map_err(|e| e.to_string())?;

    let mut hasher = Sha256::new();
    hasher.update(&decoded);
    let hash: String = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();
    let filename = format!("{}.{}", hash, ext.trim_start_matches('.'));

    let assets_dir = if let Some(p) = current_path.filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        p.parent().unwrap_or_else(|| std::path::Path::new("")).join("assets")
    } else {
        let local_data = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
        local_data.join("autosaves").join("assets")
    };

    std::fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
    let file_path = assets_dir.join(&filename);
    std::fs::write(&file_path, &decoded).map_err(|e| e.to_string())?;

    Ok(format!("assets/{}", filename))
}

#[tauri::command]
fn load_marksmen_cite_db(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let local_data = app.path().local_data_dir().map_err(|e| e.to_string())?;
    let db_path = local_data.join("com.ryancinsight.marksmen-cite").join("cite_library").join("references.json");
    if !db_path.exists() {
        return Ok("[]".to_string());
    }
    std::fs::read_to_string(&db_path).map_err(|e| e.to_string())
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
            generate_diff,
            save_file,
            save_as_format,
            autosave_file,
            load_latest_autosave,
            print_pdf,
            open_file_by_path,
            get_system_fonts,
            save_base64_asset,
            load_marksmen_cite_db,
            format_csl_citation,
            format_csl_bibliography,
            export_binder,
            execute_mail_merge,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ── Native Save ────────────────────────────────────────────────────────────

/// Write `markdown` to `current_path` if provided; otherwise open an OS Save
/// dialog (.md). Returns the absolute path that was written.
#[tauri::command]
async fn save_file(
    app: tauri::AppHandle,
    markdown: String,
    current_path: Option<String>,
) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;

    let path = if let Some(p) = current_path.filter(|p| !p.is_empty()) {
        std::path::PathBuf::from(p)
    } else {
        app.dialog()
            .file()
            .add_filter("Markdown", &["md"])
            .set_file_name("document.md")
            .blocking_save_file()
            .ok_or_else(|| "No file selected".to_string())?
            .into_path()
            .map_err(|e| e.to_string())?
    };

    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let config = Config::default();

    // For markdown, we just write the string directly
    if ext == "md" {
        std::fs::write(&path, markdown.as_bytes()).map_err(|e| e.to_string())?;
        return Ok(path.to_string_lossy().into_owned());
    }

    // For PDF, we shouldn't implicitly overwrite because it's not round-trippable losslessly
    // in the same way (can't save midway). But if the user opened a PDF, we should let them save it.
    // However, the prompt is to allow saving to DOCX/ODT/RTF etc.
    let events = parser::parse(&markdown);
    let bytes: Vec<u8> = match ext.as_str() {
        "docx" => convert_docx(events, &config, std::path::Path::new("."), None)
            .map_err(|e| e.to_string())?,
        "odt" => {
            convert_odt(&events, &config, std::path::Path::new(".")).map_err(|e| e.to_string())?
        }
        "html" => convert_html(events, &config)
            .map_err(|e| e.to_string())?
            .into_bytes(),
        "typ" => marksmen_typst::translator::translate(events, &config)
            .map_err(|e| e.to_string())?
            .into_bytes(),
        "rtf" => marksmen_rich::convert(events, &config).map_err(|e| e.to_string())?,
        "pdf" => marksmen_pdf::convert(&markdown, &config, None).map_err(|e| e.to_string())?,
        "pptx" => convert_ppt(events, &config).map_err(|e| e.to_string())?,
        "epub" => marksmen_epub::convert(events, &config).map_err(|e| e.to_string())?,
        _ => markdown.into_bytes(),
    };

    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Export to a specific format and write it to a user-chosen path via OS dialog.
#[tauri::command]
async fn save_as_format(
    app: tauri::AppHandle,
    markdown: String,
    format: String,
    doc_name: String,
) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;

    let config = Config::default();
    let events = parser::parse(&markdown);

    let (bytes, ext): (Vec<u8>, &str) = match format.as_str() {
        "docx" => (
            convert_docx(events, &config, std::path::Path::new("."), None)
                .map_err(|e| e.to_string())?,
            "docx",
        ),
        "odt" => (
            convert_odt(&events, &config, std::path::Path::new(".")).map_err(|e| e.to_string())?,
            "odt",
        ),
        "pdf" => (
            marksmen_pdf::convert(&markdown, &config, None).map_err(|e| e.to_string())?,
            "pdf",
        ),
        "html" => (
            convert_html(events, &config)
                .map_err(|e| e.to_string())?
                .into_bytes(),
            "html",
        ),
        "typst" => (
            marksmen_typst::translator::translate(events, &config)
                .map_err(|e| e.to_string())?
                .into_bytes(),
            "typ",
        ),
        "pptx" => (
            convert_ppt(events, &config).map_err(|e| e.to_string())?,
            "pptx",
        ),
        "epub" => (
            marksmen_epub::convert(events, &config).map_err(|e| e.to_string())?,
            "epub",
        ),
        "markdown" => (markdown.into_bytes(), "md"),
        _ => return Err(format!("Unknown format: {format}")),
    };

    let path = app
        .dialog()
        .file()
        .add_filter(&format.to_uppercase(), &[ext])
        .set_file_name(&format!("{doc_name}.{ext}"))
        .blocking_save_file()
        .ok_or_else(|| "No file selected".to_string())?
        .into_path()
        .map_err(|e| e.to_string())?;

    std::fs::write(&path, &bytes).map_err(|e| e.to_string())
}

/// Write markdown to an autosave shadow file in the app's local data directory.
#[tauri::command]
fn autosave_file(app: tauri::AppHandle, markdown: String, doc_name: String) -> Result<(), String> {
    use tauri::Manager;
    let local_data = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    let autosave_dir = local_data.join("autosaves");
    std::fs::create_dir_all(&autosave_dir).map_err(|e| e.to_string())?;

    let stem: String = doc_name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();

    let path = autosave_dir.join(format!("{stem}_autosave.md"));
    std::fs::write(&path, markdown.as_bytes()).map_err(|e| e.to_string())
}

/// Load the most recently modified autosave file from the local data directory.
#[tauri::command]
fn load_latest_autosave(app: tauri::AppHandle) -> Result<Option<(String, String)>, String> {
    use tauri::Manager;
    let local_data = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    let autosave_dir = local_data.join("autosaves");

    if !autosave_dir.exists() {
        return Ok(None);
    }

    let mut latest_file = None;
    let mut latest_time = std::time::UNIX_EPOCH;

    for entry in std::fs::read_dir(&autosave_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if let Ok(metadata) = entry.metadata() {
            if let Ok(modified) = metadata.modified() {
                if modified > latest_time {
                    latest_time = modified;
                    latest_file = Some(entry.path());
                }
            }
        }
    }

    if let Some(path) = latest_file {
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .replace("_autosave.md", "");
        Ok(Some((content, name)))
    } else {
        Ok(None)
    }
}

/// Render markdown → PDF → write to OS temp dir → open with native viewer.
#[tauri::command]
fn print_pdf(markdown: String) -> Result<(), String> {
    let config = Config::default();
    let bytes = marksmen_pdf::convert(&markdown, &config, None).map_err(|e| e.to_string())?;
    let path = std::env::temp_dir().join("marksmen_print.pdf");
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    let url = format!("file://{}", path.to_string_lossy().replace('\\', "/"));
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Reopen a document by its absolute path. Returns (markdown, filename, absolute_path).
/// Uses the same format-dispatch as `import_file`.
#[tauri::command]
fn open_file_by_path(path: String) -> Result<(String, String, String), String> {
    let p = std::path::Path::new(&path);
    let filename = p
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let ext = p
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    let content = match ext.as_str() {
        "md" => std::fs::read_to_string(p).map_err(|e| e.to_string()),
        "html" => {
            let h = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
            marksmen_html_read::parse_html(&h).map_err(|e| e.to_string())
        }
        "docx" => {
            let b = std::fs::read(p).map_err(|e| e.to_string())?;
            marksmen_docx_read::parse_docx(&b, None).map_err(|e| e.to_string())
        }
        "odt" => {
            let b = std::fs::read(p).map_err(|e| e.to_string())?;
            marksmen_odt_read::parse_odt(&b, None).map_err(|e| e.to_string())
        }
        "pdf" => {
            let b = std::fs::read(p).map_err(|e| e.to_string())?;
            marksmen_pdf_read::parse_pdf(&b).map_err(|e| e.to_string())
        }
        "typ" => {
            let s = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
            marksmen_typst_read::parse_typst(&s).map_err(|e| e.to_string())
        }
        "rtf" => {
            let b = std::fs::read(p).map_err(|e| e.to_string())?;
            marksmen_rich_read::parse_rtf(&b).map_err(|e| e.to_string())
        }
        "pptx" => {
            let b = std::fs::read(p).map_err(|e| e.to_string())?;
            marksmen_ppt_read::parse_pptx(&b).map_err(|e| e.to_string())
        }
        "epub" => {
            let b = std::fs::read(p).map_err(|e| e.to_string())?;
            marksmen_epub_read::parse_epub(&b).map_err(|e| e.to_string())
        }
        other => Err(format!("Unsupported extension: {other}")),
    }?;
    Ok((content, filename, p.to_string_lossy().into_owned()))
}
