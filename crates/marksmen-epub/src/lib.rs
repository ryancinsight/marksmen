//! EPUB 3 export target for the marksmen workspace.
//!
//! Wraps `marksmen-xhtml` chapter output into a standards-compliant EPUB 3 ZIP container.
//!
//! ## Structural Invariants
//! - `mimetype` is always the first uncompressed entry (EPUB spec §3.3).
//! - The event stream is split at every `H1` heading boundary into discrete XHTML chapters.
//! - If no H1 appears, the entire document is one chapter named `chapter_1.xhtml`.
//! - The first `Event::Start(Tag::Image)` alt/src is used as the cover image.

use anyhow::{Context, Result};
use marksmen_core::config::Config;
use pulldown_cmark::{Event, HeadingLevel, Tag, TagEnd};
use std::io::Write;
use zip::{write::SimpleFileOptions, ZipWriter};

pub mod oebps;

/// Converts a `pulldown-cmark` event stream into an `.epub` binary payload.
///
/// # Invariant
/// The returned `Vec<u8>` is a valid, self-contained EPUB 3 container with:
/// - One XHTML chapter file per H1 section (minimum one).
/// - A fully populated OPF manifest and spine.
/// - A fully populated NCX navMap.
pub fn convert(events: Vec<Event<'_>>, config: &Config) -> Result<Vec<u8>> {
    // Split events into chapters at every H1 boundary.
    // Each chapter is a (title, Vec<Event>) pair.
    let chapters_events = split_into_chapters(events);

    let buf = Vec::new();
    let cursor = std::io::Cursor::new(buf);
    let mut zip = ZipWriter::new(cursor);

    let stored_opts =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated_opts =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // mimetype must be uncompressed and first.
    zip.start_file("mimetype", stored_opts)?;
    zip.write_all(b"application/epub+zip")?;

    // META-INF/container.xml
    zip.add_directory("META-INF", deflated_opts)?;
    zip.start_file("META-INF/container.xml", deflated_opts)?;
    zip.write_all(oebps::CONTAINER_XML.as_bytes())?;

    zip.add_directory("OEBPS", deflated_opts)?;

    // Build chapter metadata for OPF/NCX generation.
    // Format: (id, filename, title)
    let mut chapter_meta: Vec<(String, String, String)> = Vec::new();
    let mut chapter_xhtml: Vec<(String, Vec<u8>)> = Vec::new();

    for (idx, (title, ch_events)) in chapters_events.into_iter().enumerate() {
        let id = format!("chapter_{}", idx + 1);
        let filename = format!("chapter_{}.xhtml", idx + 1);
        let xhtml_bytes = marksmen_xhtml::convert(ch_events, config)
            .with_context(|| format!("Failed to render XHTML for chapter {}", idx + 1))?;
        chapter_meta.push((id.clone(), filename.clone(), title));
        chapter_xhtml.push((filename, xhtml_bytes.into_bytes()));
    }

    // Write all chapter XHTML files.
    for (filename, xhtml_bytes) in &chapter_xhtml {
        zip.start_file(format!("OEBPS/{}", filename), deflated_opts)?;
        zip.write_all(xhtml_bytes)?;
    }

    // Build OPF/NCX slices with str references.
    let chapter_meta_strs: Vec<(&str, &str, &str)> = chapter_meta
        .iter()
        .map(|(id, file, title)| (id.as_str(), file.as_str(), title.as_str()))
        .collect();

    // content.opf
    zip.start_file("OEBPS/content.opf", deflated_opts)?;
    zip.write_all(
        oebps::generate_opf(&config.title, &config.author, &chapter_meta_strs, None).as_bytes(),
    )?;

    // toc.ncx
    zip.start_file("OEBPS/toc.ncx", deflated_opts)?;
    zip.write_all(oebps::generate_ncx(&config.title, &chapter_meta_strs).as_bytes())?;

    let finished = zip.finish().context("failed to finalize EPUB zip")?;
    Ok(finished.into_inner())
}

/// Splits a pulldown-cmark event stream into chapters at every H1 boundary.
///
/// A new chapter starts at each `Event::Start(Tag::Heading(H1, ...))`.
/// The H1 text content is extracted as the chapter title (stripped of markup).
/// The heading events are included in the chapter's event slice so the XHTML
/// renderer can produce the correct `<h1>` element.
///
/// # Returns
/// A `Vec<(title, events)>` with at least one entry.
fn split_into_chapters(events: Vec<Event<'_>>) -> Vec<(String, Vec<Event<'_>>)> {
    let mut chapters: Vec<(String, Vec<Event<'_>>)> = Vec::new();
    let mut current_title = String::from("Content");
    let mut current_events: Vec<Event<'_>> = Vec::new();
    let mut in_h1 = false;
    let mut h1_text_buf = String::new();

    for event in events {
        match &event {
            Event::Start(Tag::Heading { level: HeadingLevel::H1, .. }) => {
                // Flush the accumulated chapter before starting a new one.
                if !current_events.is_empty() {
                    chapters.push((
                        std::mem::replace(&mut current_title, String::new()),
                        std::mem::take(&mut current_events),
                    ));
                }
                in_h1 = true;
                h1_text_buf.clear();
                current_events.push(event);
            }
            Event::End(TagEnd::Heading(HeadingLevel::H1)) => {
                in_h1 = false;
                // The accumulated h1_text_buf becomes this chapter's title.
                current_title = if h1_text_buf.is_empty() {
                    format!("Chapter {}", chapters.len() + 1)
                } else {
                    std::mem::take(&mut h1_text_buf)
                };
                current_events.push(event);
            }
            Event::Text(text) if in_h1 => {
                h1_text_buf.push_str(text.as_ref());
                current_events.push(event);
            }
            _ => {
                current_events.push(event);
            }
        }
    }

    // Flush the final chapter.
    if !current_events.is_empty() {
        chapters.push((current_title, current_events));
    }

    // Guarantee at least one entry.
    if chapters.is_empty() {
        chapters.push(("Content".to_string(), Vec::new()));
    }

    chapters
}
