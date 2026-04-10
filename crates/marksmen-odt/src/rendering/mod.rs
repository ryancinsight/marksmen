use super::translation::OdtDom;
use anyhow::{Context, Result};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// The canonical OpenDocument mimetype string. Crucially, this file 
/// MUST be uncompressed and placed first in the ZIP archive.
const MIMETYPE: &str = "application/vnd.oasis.opendocument.text";

/// The structural layout manifest of the archive.
const MANIFEST_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0">
  <manifest:file-entry manifest:full-path="/" manifest:version="1.2" manifest:media-type="application/vnd.oasis.opendocument.text"/>
  <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
  <manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
  <manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>
</manifest:manifest>"#;

/// Assembles the `OdtDom` XML payloads into a compliant ZIP archive representing
/// the finalized `.odt` OpenDocument Text file, returning the raw byte stream.
pub fn assemble_archive(dom: OdtDom) -> Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);

    // Mimetype MUST be completely uncompressed (STORED) via strict `OASIS` protocol
    let stored_options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    
    // Standard compressed deflate options for XML contents
    let deflate_options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // 1. Write the uncompressed mimetype.
    zip.start_file("mimetype", stored_options)
        .context("Failed to write ODT mimetype entry")?;
    zip.write_all(MIMETYPE.as_bytes())?;

    // 2. Write the canonical XML domains.
    zip.start_file("content.xml", deflate_options)
        .context("Failed to write content.xml")?;
    zip.write_all(dom.content_xml.as_bytes())?;

    zip.start_file("styles.xml", deflate_options)
        .context("Failed to write styles.xml")?;
    zip.write_all(dom.styles_xml.as_bytes())?;

    zip.start_file("meta.xml", deflate_options)
        .context("Failed to write meta.xml")?;
    zip.write_all(dom.meta_xml.as_bytes())?;

    // 3. Write the META-INF/manifest.xml index
    zip.add_directory("META-INF", deflate_options)
        .context("Failed to create META-INF directory")?;
    zip.start_file("META-INF/manifest.xml", deflate_options)
        .context("Failed to write manifest.xml")?;
    zip.write_all(MANIFEST_XML.as_bytes())?;

    zip.finish().context("Failed to finalize ZIP archive")?;
    Ok(buffer.into_inner())
}
