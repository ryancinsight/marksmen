//! Reader for EPUB files produced by `marksmen-epub`.
//!
//! Extracts the XHTML content from the EPUB container and reconstructs a
//! Markdown representation via `marksmen-xhtml-read`.

use anyhow::{Context, Result};
use quick_xml::events::Event as XmlEvent;
use quick_xml::Reader;
use std::io::{Cursor, Read};
use zip::ZipArchive;

/// Parse an EPUB binary and reconstruct approximate Markdown.
///
/// This implementation locates the OPF rootfile via `META-INF/container.xml`,
/// parses the spine from the OPF to determine the reading order of XHTML
/// documents, and passes each document through `marksmen-xhtml-read`.
pub fn parse_epub(bytes: &[u8]) -> Result<String> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).context("EPUB is not a valid zip archive")?;

    // 1. Find the rootfile in META-INF/container.xml
    let container_xml = {
        let mut file = archive
            .by_name("META-INF/container.xml")
            .context("Missing META-INF/container.xml")?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        buf
    };

    let opf_path =
        get_opf_path(&container_xml).context("Could not find rootfile in container.xml")?;

    // 2. Read the OPF file to get manifest and spine
    let opf_xml = {
        let mut file = archive.by_name(&opf_path).context("Missing OPF rootfile")?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        buf
    };

    let opf_dir = if let Some(pos) = opf_path.rfind('/') {
        &opf_path[..pos + 1]
    } else {
        ""
    };

    let reading_order = get_reading_order(&opf_xml)?;

    let mut out = String::new();

    // 3. Extract and parse each XHTML file in reading order
    for (idx, item_href) in reading_order.iter().enumerate() {
        let full_path = format!("{}{}", opf_dir, item_href);
        let xhtml = {
            let file_res = archive.by_name(&full_path);
            if let Ok(mut file) = file_res {
                let mut buf = String::new();
                file.read_to_string(&mut buf)?;
                buf
            } else {
                continue; // Skip missing files
            }
        };

        let md = marksmen_xhtml_read::parse_xhtml(&xhtml)?;
        if !md.trim().is_empty() {
            if idx > 0 && !out.is_empty() {
                out.push_str("\n\n---\n\n");
            }
            out.push_str(&md);
        }
    }

    Ok(out)
}

fn get_opf_path(container_xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(container_xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(XmlEvent::Start(ref e)) | Ok(XmlEvent::Empty(ref e)) => {
                if e.name().as_ref() == b"rootfile" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"full-path" {
                            return String::from_utf8(attr.value.into_owned()).ok();
                        }
                    }
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

fn get_reading_order(opf_xml: &str) -> Result<Vec<String>> {
    let mut reader = Reader::from_str(opf_xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut manifest_items = std::collections::HashMap::new();
    let mut spine_idrefs = Vec::new();

    let mut in_manifest = false;
    let mut in_spine = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(XmlEvent::Start(ref e)) | Ok(XmlEvent::Empty(ref e)) => match e.name().as_ref() {
                b"manifest" => in_manifest = true,
                b"spine" => in_spine = true,
                b"item" if in_manifest => {
                    let mut id = String::new();
                    let mut href = String::new();
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"id" {
                            id = String::from_utf8(attr.value.into_owned()).unwrap_or_default();
                        } else if attr.key.as_ref() == b"href" {
                            href = String::from_utf8(attr.value.into_owned()).unwrap_or_default();
                        }
                    }
                    if !id.is_empty() && !href.is_empty() {
                        manifest_items.insert(id, href);
                    }
                }
                b"itemref" if in_spine => {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"idref" {
                            let idref =
                                String::from_utf8(attr.value.into_owned()).unwrap_or_default();
                            spine_idrefs.push(idref);
                        }
                    }
                }
                _ => {}
            },
            Ok(XmlEvent::End(ref e)) => match e.name().as_ref() {
                b"manifest" => in_manifest = false,
                b"spine" => in_spine = false,
                _ => {}
            },
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    let mut reading_order = Vec::new();
    for idref in spine_idrefs {
        if let Some(href) = manifest_items.get(&idref) {
            reading_order.push(href.clone());
        }
    }

    Ok(reading_order)
}
