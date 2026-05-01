use marksmen_core::{parsing::parser::parse, Config};

fn rtf_bytes(md: &str) -> Vec<u8> {
    crate::convert(parse(md), &Config::default()).expect("convert failed")
}
fn rtf_str(md: &str) -> String {
    String::from_utf8(rtf_bytes(md)).expect("RTF is not UTF-8")
}
fn roundtrip(md: &str) -> String {
    marksmen_rich_read::parse_rtf(&rtf_bytes(md)).expect("parse_rtf failed")
}

// ── Structural validity ────────────────────────────────────────────────────

#[test]
fn rtf_envelope_present() {
    let rtf = rtf_bytes("Hello");
    assert!(
        rtf.windows(6).any(|w| w == b"{\\rtf1"),
        "RTF1 header missing"
    );
    assert!(*rtf.last().unwrap() == b'}', "closing brace missing");
}

#[test]
fn fonttbl_present() {
    let s = rtf_str("x");
    assert!(s.contains("\\fonttbl"), "fonttbl missing");
    assert!(s.contains("Times New Roman"), "body font missing");
    assert!(s.contains("Courier New"), "mono font missing");
}

#[test]
fn colortbl_present() {
    let s = rtf_str("x");
    assert!(s.contains("\\colortbl"), "colortbl missing");
}

#[test]
fn stylesheet_present() {
    let s = rtf_str("x");
    assert!(s.contains("\\stylesheet"), "stylesheet missing");
    assert!(s.contains("heading 1"), "heading 1 style missing");
    assert!(s.contains("heading 6"), "heading 6 style missing");
}

#[test]
fn listtable_present() {
    let s = rtf_str("- item");
    assert!(s.contains("\\listtable"), "listtable missing");
    assert!(
        s.contains("\\listoverridetable"),
        "listoverridetable missing"
    );
}

#[test]
fn sectd_present() {
    let s = rtf_str("x");
    assert!(s.contains("\\sectd"), "sectd section properties missing");
}

// ── Round-trip corpus ──────────────────────────────────────────────────────

#[test]
fn plain_paragraph() {
    let back = roundtrip("Hello world");
    assert!(back.contains("Hello world"), "got: {back:?}");
}

#[test]
fn bold_control_word() {
    let s = rtf_str("**bold**");
    assert!(s.contains("\\b "), "bold control word missing: {s}");
}

#[test]
fn italic_control_word() {
    let s = rtf_str("*italic*");
    assert!(s.contains("\\i "), "italic control word missing: {s}");
}

#[test]
fn strikethrough_control_word() {
    let s = rtf_str("~~del~~");
    assert!(s.contains("\\strike "), "strike missing: {s}");
}

#[test]
fn code_span_monofont() {
    let s = rtf_str("`code`");
    assert!(s.contains("\\f1"), "mono font switch missing");
    assert!(s.contains("code"));
}

#[test]
fn headings_h1_to_h6() {
    let md = "# H1\n\n## H2\n\n### H3\n\n#### H4\n\n##### H5\n\n###### H6";
    let s = rtf_str(md);
    assert!(s.contains("\\fs48"), "H1 font size missing");
    assert!(s.contains("\\fs40"), "H2 font size missing");
    assert!(s.contains("\\fs32"), "H3 font size missing");
    assert!(s.contains("\\s1"), "H1 stylesheet ref missing");
    assert!(s.contains("\\s6"), "H6 stylesheet ref missing");
}

#[test]
fn unordered_list_uses_ls2() {
    let s = rtf_str("- alpha\n- beta");
    assert!(s.contains("\\ls2"), "unordered list \\ls2 missing");
    assert!(s.contains("\\bullet"), "bullet character missing");
    assert!(s.contains("alpha"));
}

#[test]
fn ordered_list_uses_ls1() {
    let s = rtf_str("1. first\n2. second");
    assert!(s.contains("\\ls1"), "ordered list \\ls1 missing");
}

#[test]
fn fenced_code_block_mono() {
    let s = rtf_str("```rust\nlet x = 1;\n```");
    assert!(s.contains("\\f1"), "code block mono font missing");
    assert!(s.contains("let x = 1"));
}

#[test]
fn blockquote_indent() {
    let s = rtf_str("> wisdom");
    assert!(s.contains("\\li720"), "blockquote indent missing");
}

#[test]
fn hyperlink_field() {
    let s = rtf_str("[click](https://example.com)");
    assert!(s.contains("\\field"), "hyperlink field missing");
    assert!(s.contains("https://example.com"));
    assert!(s.contains("click"));
}

#[test]
fn horizontal_rule() {
    let s = rtf_str("---");
    assert!(s.contains("\\brdrb"), "HR border missing");
}

#[test]
fn hard_line_break() {
    let s = rtf_str("line one  \nline two");
    assert!(s.contains("\\line"), "hard break missing");
}

#[test]
fn unicode_escape_non_ascii() {
    let s = crate::writer::rtf_escape("café");
    assert!(s.contains("\\u233?"), "Unicode escape for é missing: {s}");
}

#[test]
fn table_cellx_per_column() {
    let md = "| A | B | C |\n|---|---|---|\n| 1 | 2 | 3 |";
    let s = rtf_str(md);
    assert!(s.contains("\\trowd"), "trowd missing");
    assert!(s.contains("\\cellx"), "cellx missing");
    assert!(s.contains("\\row"), "row marker missing");
    // 3-column: body=9360, each col=3120 → \cellx3120 \cellx6240 \cellx9360
    assert!(
        s.contains("\\cellx3120"),
        "per-col cellx width wrong for 3 cols"
    );
}

#[test]
fn table_two_columns() {
    let md = "| X | Y |\n|---|---|\n| a | b |";
    let s = rtf_str(md);
    // 2-column: each = 4680
    assert!(s.contains("\\cellx4680"), "2-col cellx wrong");
}

#[test]
fn inline_math_momath() {
    let s = rtf_str("$E=mc^2$");
    assert!(
        s.contains("\\momath") || s.contains("\\f1"),
        "math encoding missing"
    );
}

#[test]
fn display_math_block() {
    let s = rtf_str("$$\\int_0^\\infty$$");
    assert!(
        s.contains("\\li360") || s.contains("\\momath"),
        "display math block missing"
    );
}

#[test]
fn image_external_url_placeholder() {
    let s = rtf_str("![alt text](https://example.com/img.png)");
    assert!(
        s.contains("Figure") || s.contains("alt text"),
        "image placeholder missing"
    );
}

#[test]
fn image_data_uri_pict() {
    // Minimal 1×1 white PNG (67 bytes)
    let data_uri = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwADhQGAWjR9awAAAABJRU5ErkJggg==";
    let md = format!("![tiny]({data_uri})");
    let s = rtf_str(&md);
    assert!(
        s.contains("\\pict"),
        "\\pict block missing for data-URI image"
    );
    assert!(s.contains("\\pngblip"), "\\pngblip missing");
}

#[test]
fn footnote_reference_superscript() {
    let s = rtf_str("Text[^1]\n\n[^1]: The note.");
    assert!(s.contains("\\super"), "footnote superscript missing");
}
