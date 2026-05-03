use super::translation::OdtDom;
use anyhow::{Context, Result};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// The canonical OpenDocument mimetype string. Crucially, this file
/// MUST be uncompressed and placed first in the ZIP archive.
const MIMETYPE: &str = "application/vnd.oasis.opendocument.text";

fn get_manifest_xml(math_objects_count: usize, images: &[(String, Vec<u8>)]) -> String {
    let mut manifest = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0">
  <manifest:file-entry manifest:full-path="/" manifest:version="1.2" manifest:media-type="application/vnd.oasis.opendocument.text"/>
  <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
  <manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
  <manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>"#.to_string();

    for i in 1..=math_objects_count {
        manifest.push_str(&format!(
            "\n  <manifest:file-entry manifest:full-path=\"Object {}/\" manifest:media-type=\"application/vnd.oasis.opendocument.formula\"/>",
            i
        ));
        manifest.push_str(&format!(
            "\n  <manifest:file-entry manifest:full-path=\"Object {}/content.xml\" manifest:media-type=\"text/xml\"/>",
            i
        ));
    }

    if !images.is_empty() {
        manifest.push_str(
            "\n  <manifest:file-entry manifest:full-path=\"Pictures/\" manifest:media-type=\"\"/>",
        );
        for (image_id, _data) in images {
            let ext = image_id.split('.').next_back().unwrap_or("png");
            let mime_type = match ext.to_lowercase().as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "gif" => "image/gif",
                "svg" => "image/svg+xml",
                _ => "image/png",
            };
            manifest.push_str(&format!(
                "\n  <manifest:file-entry manifest:full-path=\"{}\" manifest:media-type=\"{}\"/>",
                image_id, mime_type
            ));
        }
    }

    manifest.push_str("\n</manifest:manifest>");
    manifest
}

/// Assembles the `OdtDom` XML payloads into a compliant ZIP archive representing
/// the finalized `.odt` OpenDocument Text file, returning the raw byte stream.
pub fn assemble_archive(dom: OdtDom) -> Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);

    // Mimetype MUST be completely uncompressed (STORED) via strict `OASIS` protocol
    let stored_options =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    // Standard compressed deflate options for XML contents
    let deflate_options =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

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

    for (idx, math_string) in dom.math_objects.iter().enumerate() {
        let object_num = idx + 1;
        let dir_name = format!("Object {}", object_num);
        zip.add_directory(&dir_name, deflate_options)
            .with_context(|| format!("Failed to create {} directory", dir_name))?;

        // Wrap raw MathML inside a root element with standard MathML XML namespaces
        let mathml_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<math xmlns="http://www.w3.org/1998/Math/MathML" display="block">
{}
</math>"#,
            math_string.replace(r#"xmlns="http://www.w3.org/1998/Math/MathML""#, "")
        );

        zip.start_file(format!("{}/content.xml", dir_name), deflate_options)
            .with_context(|| format!("Failed to write {}/content.xml", dir_name))?;
        zip.write_all(mathml_content.as_bytes())?;
    }

    // Write standard embedded pictures
    if !dom.images.is_empty() {
        zip.add_directory("Pictures", stored_options)
            .context("Failed to create Pictures directory")?;
        for (image_id, data) in &dom.images {
            zip.start_file(image_id.clone(), stored_options)
                .with_context(|| format!("Failed to write {}", image_id))?;
            zip.write_all(data)?;
        }
    }

    // 3. Write the META-INF/manifest.xml index
    zip.add_directory("META-INF", deflate_options)
        .context("Failed to create META-INF directory")?;
    zip.start_file("META-INF/manifest.xml", deflate_options)
        .context("Failed to write manifest.xml")?;
    zip.write_all(get_manifest_xml(dom.math_objects.len(), &dom.images).as_bytes())?;

    zip.finish().context("Failed to finalize ZIP archive")?;
    Ok(buffer.into_inner())
}
