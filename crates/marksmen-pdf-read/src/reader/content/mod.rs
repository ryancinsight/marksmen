//! Page content stream aggregation.
//!
//! A PDF page's `/Contents` key may be a direct stream reference, a reference
//! that resolves to an array, or a direct array of references. This module
//! normalizes all cases into concatenated decompressed bytes.

use anyhow::{Context, Result};
use lopdf::{Document, Object, ObjectId};

/// Aggregate all content stream bytes for a page into one contiguous buffer.
///
/// Streams are concatenated in order with a single newline separator so that
/// operators split across streams remain parseable.
pub fn aggregate_page_streams(doc: &Document, page_id: ObjectId) -> Result<Vec<u8>> {
    let page = doc
        .get_dictionary(page_id)
        .context("Failed to get page dictionary")?;

    let stream_ids = resolve_content_ids(doc, page)?;

    let mut buf: Vec<u8> = Vec::new();
    for sid in stream_ids {
        let obj = doc
            .get_object(sid)
            .context("Failed to get content stream object")?;
        let stream = match obj {
            Object::Stream(s) => s,
            _ => continue,
        };
        let raw = stream
            .decompressed_content()
            .context("Failed to decompress content stream")?;
        buf.extend_from_slice(&raw);
        buf.push(b'\n'); // separation guard
    }

    Ok(buf)
}

fn resolve_content_ids(doc: &Document, page: &lopdf::Dictionary) -> Result<Vec<ObjectId>> {
    let raw = match page.get(b"Contents") {
        Ok(o) => o.clone(),
        Err(_) => return Ok(Vec::new()),
    };

    let ids = match raw {
        Object::Reference(id) => {
            match doc.get_object(id).context("Resolving Contents reference")? {
                Object::Array(arr) => arr.iter().filter_map(|o| o.as_reference().ok()).collect(),
                Object::Stream(_) => vec![id],
                _ => Vec::new(),
            }
        }
        Object::Array(arr) => arr.iter().filter_map(|o| o.as_reference().ok()).collect(),
        _ => Vec::new(),
    };

    Ok(ids)
}
