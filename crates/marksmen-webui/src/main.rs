use axum::{
    routing::post,
    Router, Json,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tower_http::services::ServeDir;
use tower_http::cors::CorsLayer;
use std::io::{Cursor, Read};

use marksmen_core::config::Config;
use marksmen_core::parsing::parser;
use marksmen_html::convert as convert_html;
use marksmen_typst::translator::translate as translate_typst;
use marksmen_docx::translation::document::convert as convert_docx;
use marksmen_odt::translate_and_render as convert_odt;
use marksmen_docx_read::parse_docx as read_docx;
use marksmen_odt_read::parse_odt as read_odt;

/// Roundtrip request: the raw Markdown string from the editor.
#[derive(Deserialize)]
struct InspectRequest {
    markdown: String,
}

/// Roundtrip response carrying previews for every output format.
///
/// All preview fields are self-contained HTML strings injected into `<iframe srcdoc>`,
/// except `preview_typst_svg` which is a base64-encoded SVG data URI and
/// `preview_pdf_b64` which is a base64-encoded PDF blob.
#[derive(Serialize)]
struct InspectResponse {
    /// Source-level outputs (for the structured tabs).
    ast: String,
    html_src: String,
    typst_src: String,
    docx_xml: String,
    odt_xml: String,

    /// Visual previews.
    preview_html: String,
    preview_docx: String,
    preview_odt: String,
    /// Full multi-page SVG string (already escaped-safe as an HTML snippet).
    preview_typst_svg: String,
    /// Base64-encoded PDF for embedding in an <embed> data URI.
    preview_pdf_b64: String,
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .nest_service("/", ServeDir::new("crates/marksmen-webui/static"))
        .route("/api/inspect", post(handle_inspect))
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("marksmen-webui running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_inspect(Json(payload): Json<InspectRequest>) -> Json<InspectResponse> {
    let markdown = payload.markdown;

    // ── Parse front-matter and build event stream ─────────────────────────
    let (body, fm_config) = match marksmen_core::config::frontmatter::parse_frontmatter(&markdown) {
        Ok(res) => res,
        Err(_) => (markdown.as_str(), marksmen_core::config::FrontMatterConfig::default()),
    };
    let config = Config::default().merge_frontmatter(&fm_config);
    let events = parser::parse(body);

    // ── AST trace ─────────────────────────────────────────────────────────
    let ast = events.iter().map(|e| format!("{:?}\n", e)).collect::<String>();

    // ── HTML ──────────────────────────────────────────────────────────────
    let html_src = convert_html(events.clone(), &config)
        .unwrap_or_else(|e| format!("<!-- HTML Error: {} -->", e));

    // Wrap the raw fragment in a minimal document styled for iframe preview.
    let preview_html = build_preview_doc(&html_src);

    // ── Typst ─────────────────────────────────────────────────────────────
    let typst_src = translate_typst(events.clone(), &config)
        .unwrap_or_else(|e| format!("// Typst Error: {}", e));

    // Compile Typst → per-page SVGs concatenated into one HTML document.
    let preview_typst_svg = compile_typst_to_svg_html(&typst_src, &config);

    // ── DOCX ──────────────────────────────────────────────────────────────
    let (docx_xml, preview_docx) = match convert_docx(events.clone(), &config, std::path::Path::new(".")) {
        Ok(bytes) => {
            let xml = extract_zip_file(&bytes, "word/document.xml")
                .unwrap_or_else(|e| format!("DOCX Extract Error: {}", e));
            // Reconstruct via read → HTML for visual preview.
            let preview = match read_docx(&bytes, None) {
                Ok(md) => {
                    let rt_events = parser::parse(&md);
                    let rt_html = convert_html(rt_events, &config)
                        .unwrap_or_else(|e| format!("<!-- DOCX RT HTML Error: {} -->", e));
                    build_preview_doc(&rt_html)
                }
                Err(e) => build_error_doc(&format!("DOCX read error: {}", e)),
            };
            (xml, preview)
        }
        Err(e) => (
            format!("DOCX Build Error: {}", e),
            build_error_doc(&format!("DOCX build error: {}", e)),
        ),
    };

    // ── ODT ───────────────────────────────────────────────────────────────
    let (odt_xml, preview_odt) = match convert_odt(&events, &config, std::path::Path::new(".")) {
        Ok(bytes) => {
            let xml = extract_zip_file(&bytes, "content.xml")
                .unwrap_or_else(|e| format!("ODT Extract Error: {}", e));
            let preview = match read_odt(&bytes) {
                Ok(md) => {
                    let rt_events = parser::parse(&md);
                    let rt_html = convert_html(rt_events, &config)
                        .unwrap_or_else(|e| format!("<!-- ODT RT HTML Error: {} -->", e));
                    build_preview_doc(&rt_html)
                }
                Err(e) => build_error_doc(&format!("ODT read error: {}", e)),
            };
            (xml, preview)
        }
        Err(e) => (
            format!("ODT Build Error: {}", e),
            build_error_doc(&format!("ODT build error: {}", e)),
        ),
    };

    // ── PDF ───────────────────────────────────────────────────────────────
    let preview_pdf_b64 = match marksmen_pdf::convert(&markdown, &config, None) {
        Ok(bytes) => base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes),
        Err(e) => format!("PDF Error: {}", e),
    };

    Json(InspectResponse {
        ast,
        html_src,
        typst_src,
        docx_xml,
        odt_xml,
        preview_html,
        preview_docx,
        preview_odt,
        preview_typst_svg,
        preview_pdf_b64,
    })
}

// ── Private helpers ──────────────────────────────────────────────────────────

/// Compile a Typst source string to a sequence of SVG pages and wrap
/// them into a self-contained HTML preview document.
fn compile_typst_to_svg_html(typst_src: &str, _config: &Config) -> String {
    use marksmen_render::MarksmenWorld;
    use typst::World;

    let world = match MarksmenWorld::new(typst_src, None) {
        Ok(w) => w,
        Err(e) => return build_error_doc(&format!("Typst world error: {}", e)),
    };

    let compile_result = typst::compile(&world);
    let document: typst::layout::PagedDocument = match compile_result.output {
        Ok(doc) => doc,
        Err(ref diagnostics) => {
            let msgs: Vec<String> = diagnostics.iter().map(|d| {
                let mut loc = String::new();
                if let Some(id) = d.span.id() {
                    if let Ok(src) = world.source(id) {
                        if let Some(range) = src.range(d.span) {
                            let s = range.start.saturating_sub(30);
                            let e = (range.end + 30).min(src.text().len());
                            loc = format!(" near `{}`", &src.text()[s..e]);
                        }
                    }
                }
                format!("{:?}{}: {}", d.severity, loc, d.message)
            }).collect();
            return build_error_doc(&format!("Typst compile error:\n{}", msgs.join("\n")));
        }
    };

    // Render every page as SVG and stitch them into one scrollable HTML doc.
    let mut pages_html = String::new();
    for page in document.pages.iter() {
        let svg = typst_svg::svg(page);
        pages_html.push_str(r#"<div class="typst-page">"#);
        pages_html.push_str(&svg);
        pages_html.push_str("</div>\n");
    }

    format!(
        r#"<!DOCTYPE html><html><head><meta charset="UTF-8">
<style>
  body {{ margin: 0; background: #f5f5f5; }}
  .typst-page {{ background: white; margin: 16px auto; box-shadow: 0 2px 8px rgba(0,0,0,0.15); width: fit-content; }}
  .typst-page svg {{ display: block; }}
</style></head><body>{}</body></html>"#,
        pages_html
    )
}

/// Wrap an HTML fragment in a minimal preview document.
fn build_preview_doc(fragment: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="UTF-8">
<style>
  body {{ font-family: Georgia, serif; line-height: 1.7; max-width: 800px; margin: 24px auto; padding: 0 24px; color: #1a1a1a; }}
  h1, h2, h3, h4 {{ font-family: 'Arial', sans-serif; color: #111; }}
  code {{ background: #f0f0f0; padding: 2px 5px; border-radius: 3px; font-family: monospace; }}
  pre {{ background: #f0f0f0; padding: 12px; border-radius: 4px; overflow-x: auto; }}
  table {{ border-collapse: collapse; width: 100%; }}
  th, td {{ border: 1px solid #ccc; padding: 8px 12px; }}
  th {{ background: #f5f5f5; font-weight: 600; }}
  blockquote {{ border-left: 4px solid #ccc; margin: 0; padding-left: 16px; color: #555; }}
  img {{ max-width: 100%; }}
  math {{ font-size: 1.1em; }}
</style></head><body>{}</body></html>"#,
        fragment
    )
}

/// Build a minimal error document for iframe preview slots.
fn build_error_doc(msg: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="UTF-8">
<style>body {{ font-family: monospace; padding: 24px; background: #fff0f0; color: #c00; }}</style>
</head><body><pre>{}</pre></body></html>"#,
        marksmen_xml::escape(msg)
    )
}

fn extract_zip_file(bytes: &[u8], target: &str) -> anyhow::Result<String> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;
    let mut file = archive.by_name(target)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Indent XML for readability.
    let mut formatted = String::with_capacity(contents.len() + 1024);
    let mut indent: usize = 0;
    for token in contents.replace("><", ">\n<").lines() {
        if token.starts_with("</") {
            indent = indent.saturating_sub(2);
            formatted.push_str(&" ".repeat(indent));
            formatted.push_str(token);
            formatted.push('\n');
        } else if token.starts_with('<') && token.ends_with("/>") {
            formatted.push_str(&" ".repeat(indent));
            formatted.push_str(token);
            formatted.push('\n');
        } else if token.starts_with('<') && !token.starts_with("<?") {
            formatted.push_str(&" ".repeat(indent));
            formatted.push_str(token);
            formatted.push('\n');
            indent += 2;
        } else {
            formatted.push_str(&" ".repeat(indent));
            formatted.push_str(token);
            formatted.push('\n');
        }
    }
    Ok(formatted)
}
