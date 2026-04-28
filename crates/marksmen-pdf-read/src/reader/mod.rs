//! Native PDF reader — public entry point.
//!
//! `extract_pages()` walks every page via `lopdf`, resolves resources and font
//! dictionaries, processes content stream operators through a full `GraphicsState`
//! machine, and returns `RichSpan`s for every decoded glyph.

pub mod content;
pub mod font;
pub mod graphics;
pub mod model;

use anyhow::{Context, Result};
use lopdf::{Document, Object};

use content::aggregate_page_streams;
use graphics::operators::process_ops;
use graphics::state::GraphicsState;
use model::cluster::cluster_to_markdown;
use model::span::RichSpan;

/// Extract all `RichSpan`s from every page of a PDF document.
pub fn extract_pages(doc: &Document) -> Result<(Vec<RichSpan>, Vec<GraphicRect>)> {
    let page_ids: Vec<_> = doc.get_pages().into_iter().collect();
    let mut all_spans: Vec<RichSpan> = Vec::new();
    let mut all_rects: Vec<GraphicRect> = Vec::new();

    for (page_num, page_id) in page_ids {
        let (spans, rects) = extract_one_page(doc, page_id, page_num).unwrap_or_else(|e| {
            tracing::warn!(page=page_num, error=%e, "Failed to extract page");
            (Vec::new(), Vec::new())
        });
        all_spans.extend(spans);
        all_rects.extend(rects);
    }

    Ok((all_spans, all_rects))
}

/// Convert a PDF byte slice directly to Markdown via the native reader.
pub fn pdf_to_markdown(bytes: &[u8]) -> Result<String> {
    let doc = Document::load_mem(bytes).context("Failed to parse PDF bytes")?;
    let (spans, rects) = extract_pages(&doc)?;
    if spans.is_empty() {
        return Ok(String::new());
    }
    Ok(cluster_to_markdown(spans, rects))
}

// ─── Per-page extraction ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GraphicRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub page: u32,
}

fn extract_one_page(
    doc: &Document,
    page_id: lopdf::ObjectId,
    page_num: u32,
) -> Result<(Vec<RichSpan>, Vec<GraphicRect>)> {
    let page = doc
        .get_dictionary(page_id)
        .context("Failed to get page dictionary")?;

    // Resolve page resource dictionary.
    let resources = page
        .get(b"Resources")
        .and_then(|o| match o {
            Object::Dictionary(d) => Ok(d.clone()),
            Object::Reference(id) => doc.get_dictionary(*id).cloned(),
            _ => Err(lopdf::Error::DictKey),
        })
        .unwrap_or_default();

    // Aggregate + decode content streams.
    let raw = aggregate_page_streams(doc, page_id)?;
    if raw.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let content = lopdf::content::Content::decode(&raw)
        .context("Failed to decode content stream operations")?;

    // Process all operations.
    let mut state = GraphicsState::default();
    let mut spans: Vec<RichSpan> = Vec::new();
    let mut rects: Vec<GraphicRect> = Vec::new();

    process_ops(
        &content.operations,
        &mut state,
        &resources,
        doc,
        page_num,
        &mut spans,
        &mut rects,
    );

    Ok((spans, rects))
}
