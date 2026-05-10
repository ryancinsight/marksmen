use marksmen_core::config::Config;
use marksmen_core::parsing::parser;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

/// Converts Markdown text into structural HTML.
#[wasm_bindgen]
pub fn md_to_html(markdown: &str) -> Result<String, JsValue> {
    use marksmen_core::config::frontmatter::parse_frontmatter;

    let (body, fm) = parse_frontmatter(markdown).unwrap_or((markdown, Default::default()));
    let config = Config::default().merge_frontmatter(&fm);

    let events = parser::parse(body);
    match marksmen_html::convert(&events, &config) {
        Ok(html) => Ok(html),
        Err(e) => Err(JsValue::from_str(&e.to_string())),
    }
}

/// Converts structural HTML back into Markdown.
#[wasm_bindgen]
pub fn html_to_md(html: &str) -> Result<String, JsValue> {
    match marksmen_html_read::parse_html(html) {
        Ok(md) => Ok(md),
        Err(e) => Err(JsValue::from_str(&e.to_string())),
    }
}
/// Exports Markdown to the specified format (docx, pptx, epub, rtf, typst), returning the bytes.
#[wasm_bindgen]
pub fn export_document(markdown: &str, format: &str) -> Result<js_sys::Uint8Array, JsValue> {
    use marksmen_core::config::frontmatter::parse_frontmatter;

    let (body, fm) = parse_frontmatter(markdown).unwrap_or((markdown, Default::default()));
    let config = Config::default().merge_frontmatter(&fm);
    let events = marksmen_core::parsing::parser::parse(body);

    let bytes = match format {
        "docx" => marksmen_docx::translation::document::convert(
            &events,
            &config,
            std::path::Path::new(""),
            None,
        )
        .map_err(|e| e.to_string())?,
        "pptx" => marksmen_ppt::convert(&events, &config).map_err(|e| e.to_string())?,
        "epub" => marksmen_epub::convert(&events, &config).map_err(|e| e.to_string())?,
        "typst" | "typ" => marksmen_typst::translator::translate(&events, &config)
            .map_err(|e| e.to_string())?
            .into_bytes(),
        "html" | "htm" => marksmen_html::convert(&events, &config)
            .map_err(|e| e.to_string())?
            .into_bytes(),
        _ => {
            return Err(JsValue::from_str(&format!(
                "Unsupported WASM export format: {}",
                format
            )))
        }
    };

    Ok(js_sys::Uint8Array::from(bytes.as_slice()))
}

/// Formats a CSL citation using marksmen-csl.
#[wasm_bindgen]
pub fn format_csl_citation(citation_json: &str, style_xml: &str) -> Result<String, JsValue> {
    use marksmen_csl::engine::{evaluate_layout, Context};
    use marksmen_csl::schema::Style;

    let refs: Vec<marksmen_csl::model::Reference> = serde_json::from_str(citation_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid citation JSON: {}", e)))?;

    if refs.is_empty() {
        return Ok("".to_string());
    }

    // Parse style or use dummy
    let style: Style = if style_xml.is_empty() {
        quick_xml::de::from_str(r#"<style class="in-text" version="1.0"><citation><layout><text variable="title"/></layout></citation></style>"#)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse default style: {}", e)))?
    } else {
        quick_xml::de::from_str(style_xml)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse CSL style: {}", e)))?
    };

    let mut outputs = Vec::new();
    for r in &refs {
        let ctx = Context::new(&style, r);
        if let Some(layout) = &style.citation.layout {
            outputs.push(evaluate_layout(layout, &ctx));
        } else {
            outputs.push(r.title.clone().unwrap_or_default());
        }
    }

    Ok(outputs.join("; "))
}

/// Formats a CSL bibliography using marksmen-csl.
#[wasm_bindgen]
pub fn format_csl_bibliography(citations_json: &str, style_xml: &str) -> Result<String, JsValue> {
    use marksmen_csl::engine::{evaluate_layout, Context};
    use marksmen_csl::schema::Style;

    let refs: Vec<marksmen_csl::model::Reference> = serde_json::from_str(citations_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid citations JSON: {}", e)))?;

    let style: Style = if style_xml.is_empty() {
        quick_xml::de::from_str(r#"<style class="in-text" version="1.0"><citation><layout><text variable="title"/></layout></citation></style>"#)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse default style: {}", e)))?
    } else {
        quick_xml::de::from_str(style_xml)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse CSL style: {}", e)))?
    };

    let mut outputs = Vec::new();
    for r in &refs {
        let ctx = Context::new(&style, r);
        if let Some(bib) = &style.bibliography {
            if let Some(layout) = &bib.layout {
                outputs.push(format!(
                    "<div class=\"csl-entry\">{}</div>",
                    evaluate_layout(layout, &ctx)
                ));
                continue;
            }
        }
        outputs.push(format!(
            "<div class=\"csl-entry\">{}</div>",
            r.title.clone().unwrap_or_default()
        ));
    }

    Ok(outputs.join("\n"))
}

/// Executes a mail merge operation over a template and CSV data, returning the generated document.
#[wasm_bindgen]
pub fn execute_mail_merge(
    template_markdown: &str,
    csv_data: &str,
    format: &str,
) -> Result<js_sys::Uint8Array, JsValue> {
    use marksmen_core::config::Config;
    use marksmen_core::parsing::combine::AstConcatenator;
    use marksmen_core::parsing::mailmerge::process_ast;
    use marksmen_core::parsing::parser;
    use std::collections::HashMap;

    let config = Config::default();

    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(csv_data.as_bytes());

    let headers = reader
        .headers()
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .clone();

    // Parse the template ONCE
    let template_events = parser::parse(template_markdown);

    let mut concat = AstConcatenator::new();
    let mut num_records = 0;

    for (idx, result) in reader.records().enumerate() {
        let record = result.map_err(|e| JsValue::from_str(&e.to_string()))?;
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
        return Err(JsValue::from_str("No data records found in CSV."));
    }

    let combined_events = concat.build();

    let bytes = match format {
        "docx" => marksmen_docx::translation::document::convert(
            &combined_events,
            &config,
            std::path::Path::new(""),
            None,
        )
        .map_err(|e| e.to_string())?,
        "pptx" => marksmen_ppt::convert(&combined_events, &config).map_err(|e| e.to_string())?,
        "epub" => marksmen_epub::convert(&combined_events, &config).map_err(|e| e.to_string())?,
        "typst" | "typ" => marksmen_typst::translator::translate(&combined_events, &config)
            .map_err(|e| e.to_string())?
            .into_bytes(),
        "html" | "htm" => marksmen_html::convert(&combined_events, &config)
            .map_err(|e| e.to_string())?
            .into_bytes(),
        _ => {
            return Err(JsValue::from_str(&format!(
                "Unsupported WASM merge format: {}",
                format
            )))
        }
    };

    Ok(js_sys::Uint8Array::from(bytes.as_slice()))
}

/// Generates a structural diff payload between two Markdown documents.
#[wasm_bindgen]
pub fn generate_diff(old_md: &str, new_md: &str) -> Result<String, JsValue> {
    let html = marksmen_diff::diff_markdown(old_md, new_md);
    Ok(html)
}
