//! OEBPS manifest and NCX generation for EPUB 3.
//!
//! ## Invariants
//! - `generate_opf` produces a valid OPF 3.0 package with a manifest entry
//!   for every chapter file and an optional cover-image entry.
//! - `generate_ncx` produces a valid NCX 2005-1 navMap with one navPoint per chapter.

pub const CONTAINER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

/// Generates an OPF 3.0 package document for the given chapters.
///
/// # Parameters
/// - `title`, `author`: document metadata.
/// - `chapters`: slice of `(id, filename, title)` tuples.
/// - `cover_image_filename`: optional cover image filename within `OEBPS/`.
pub fn generate_opf(
    title: &str,
    author: &str,
    chapters: &[(&str, &str, &str)],
    cover_image_filename: Option<&str>,
) -> String {
    let title_esc = marksmen_xml::escape(title);
    let author_esc = marksmen_xml::escape(author);

    let mut manifest_items = String::new();
    manifest_items.push_str(
        "    <item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\n",
    );
    if let Some(cover) = cover_image_filename {
        let ext = std::path::Path::new(cover)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png");
        let media_type = match ext {
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            _ => "image/png",
        };
        manifest_items.push_str(&format!(
            "    <item id=\"cover-image\" href=\"{}\" media-type=\"{}\" properties=\"cover-image\"/>\n",
            cover, media_type
        ));
    }
    for (id, filename, _title) in chapters {
        manifest_items.push_str(&format!(
            "    <item id=\"{}\" href=\"{}\" media-type=\"application/xhtml+xml\"/>\n",
            id, filename
        ));
    }

    let mut spine_items = String::new();
    for (id, _, _) in chapters {
        spine_items.push_str(&format!("    <itemref idref=\"{}\"/>\n", id));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{}</dc:title>
    <dc:creator>{}</dc:creator>
    <dc:language>en</dc:language>
    <dc:identifier id="BookId">urn:uuid:marksmen-epub</dc:identifier>
  </metadata>
  <manifest>
{}  </manifest>
  <spine toc="ncx">
{}  </spine>
</package>"#,
        title_esc, author_esc, manifest_items, spine_items
    )
}

/// Generates an NCX 2005-1 navigation document with one navPoint per chapter.
pub fn generate_ncx(title: &str, chapters: &[(&str, &str, &str)]) -> String {
    let title_esc = marksmen_xml::escape(title);

    let mut nav_points = String::new();
    for (idx, (_id, filename, ch_title)) in chapters.iter().enumerate() {
        let ch_title_esc = marksmen_xml::escape(ch_title);
        nav_points.push_str(&format!(
            "    <navPoint id=\"navPoint-{}\" playOrder=\"{}\">\n      <navLabel><text>{}</text></navLabel>\n      <content src=\"{}\"/>\n    </navPoint>\n",
            idx + 1,
            idx + 1,
            ch_title_esc,
            filename
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head>
    <meta name="dtb:uid" content="urn:uuid:marksmen-epub"/>
    <meta name="dtb:depth" content="1"/>
    <meta name="dtb:totalPageCount" content="0"/>
    <meta name="dtb:maxPageNumber" content="0"/>
  </head>
  <docTitle><text>{}</text></docTitle>
  <navMap>
{}  </navMap>
</ncx>"#,
        title_esc, nav_points
    )
}
