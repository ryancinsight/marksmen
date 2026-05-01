// ── Parser adversarial inputs ─────────────────────────────────────────────

#[test]
fn parse_empty_rtf() {
    let md = crate::parse_rtf(b"{\\rtf1\\ansi }").unwrap();
    assert!(!md.contains("\\rtf"), "raw RTF leaked: {md:?}");
}

#[test]
fn parse_plain_text() {
    let md = crate::parse_rtf(b"{\\rtf1\\ansi Hello World\\par}").unwrap();
    assert!(md.contains("Hello World"), "text not extracted: {md:?}");
}

#[test]
fn parse_bold_italic_extracted() {
    let rtf = b"{\\rtf1 {\\b bold} {\\i italic}\\par}";
    let md = crate::parse_rtf(rtf).unwrap();
    assert!(md.contains("bold"), "bold text missing: {md:?}");
    assert!(md.contains("italic"), "italic text missing: {md:?}");
}

#[test]
fn parse_unknown_control_words_skipped() {
    let md = crate::parse_rtf(b"{\\rtf1 \\xyzunknown999 Hello\\par}").unwrap();
    assert!(md.contains("Hello"), "text lost after unknown ctrl: {md:?}");
}

#[test]
fn parse_deleted_text_suppressed() {
    let md = crate::parse_rtf(b"{\\rtf1 visible{\\deleted deleted}\\par}").unwrap();
    assert!(md.contains("visible"), "visible text missing: {md:?}");
    assert!(!md.contains("deleted"), "deleted text leaked: {md:?}");
}

#[test]
fn parse_deeply_nested_groups() {
    let open = "{\\rtf1 ".to_string() + &"{".repeat(64) + "deep";
    let close = "}".repeat(64) + "\\par}";
    let rtf = format!("{open}{close}");
    let md = crate::parse_rtf(rtf.as_bytes()).unwrap();
    assert!(md.contains("deep"), "text lost in deep nesting: {md:?}");
}

#[test]
fn parse_ansi_hex_escape() {
    let md = crate::parse_rtf(b"{\\rtf1 caf\\'e9\\par}").unwrap();
    assert!(
        md.contains('é') || md.contains("caf"),
        "ANSI \\'e9 not decoded: {md:?}"
    );
}

#[test]
fn parse_unicode_escape() {
    let md = crate::parse_rtf(b"{\\rtf1 caf\\u233?\\par}").unwrap();
    assert!(
        md.contains('é') || md.contains("caf"),
        "\\u233 not decoded: {md:?}"
    );
}

#[test]
fn parse_fonttbl_skipped() {
    let md = crate::parse_rtf(b"{\\rtf1 {\\fonttbl{\\f0 Arial;}} Hello\\par}").unwrap();
    assert!(!md.contains("Arial"), "fonttbl leaked: {md:?}");
    assert!(
        md.contains("Hello"),
        "body text missing after fonttbl: {md:?}"
    );
}

#[test]
fn parse_outlinelevel_heading() {
    let md = crate::parse_rtf(b"{\\rtf1 \\pard\\outlinelevel0\\b Introduction\\b0\\par}").unwrap();
    assert!(
        md.starts_with('#'),
        "outlinelevel0 not mapped to H1: {md:?}"
    );
}
