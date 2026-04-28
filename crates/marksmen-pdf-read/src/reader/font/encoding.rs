//! Standard PDF character encodings.
//!
//! Implements the three encoding tables defined in ISO 32000-1 Annex D:
//! - WinAnsiEncoding (Windows-1252 with PDF-specified substitutions)
//! - MacRomanEncoding
//! - StandardEncoding (PostScript standard encoding)
//!
//! Each table maps a byte value (0x00–0xFF) to a Unicode `char`.
//! Undefined slots map to `'\0'` (treated as gap / not-defined).

/// Which named encoding to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingKind {
    WinAnsi,
    MacRoman,
    Standard,
    /// Latin-1 (ISO 8859-1) — direct Unicode identity for 0x00–0xFF.
    MacExpert,
}

/// Decode a single byte using the specified named encoding.
/// Returns `'\0'` for undefined slots.
pub fn decode_byte(enc: EncodingKind, byte: u8) -> char {
    match enc {
        EncodingKind::WinAnsi => WIN_ANSI[byte as usize],
        EncodingKind::MacRoman => MAC_ROMAN[byte as usize],
        EncodingKind::Standard => STANDARD[byte as usize],
        EncodingKind::MacExpert => char::from_u32(byte as u32).unwrap_or('\0'),
    }
}

/// Resolve a named encoding string (from PDF `/Encoding` name) to an `EncodingKind`.
pub fn encoding_from_name(name: &str) -> Option<EncodingKind> {
    match name {
        "WinAnsiEncoding" => Some(EncodingKind::WinAnsi),
        "MacRomanEncoding" => Some(EncodingKind::MacRoman),
        "StandardEncoding" => Some(EncodingKind::Standard),
        "MacExpertEncoding" => Some(EncodingKind::MacExpert),
        // PDFDocEncoding is a superset of Latin-1 for metadata strings; treat as WinAnsi.
        "PDFDocEncoding" => Some(EncodingKind::WinAnsi),
        _ => None,
    }
}

// ─── WinAnsiEncoding ────────────────────────────────────────────────────────
// ISO 32000-1 Annex D.2.  0x00–0x1F and 0x7F are undefined ('\0').
// 0x80–0x9F use Windows-1252 mapping; 0xA0–0xFF are Latin-1.
#[rustfmt::skip]
const WIN_ANSI: [char; 256] = [
    // 0x00–0x1F (control, undefined in WinAnsiEncoding)
    '\0','\0','\0','\0','\0','\0','\0','\0', '\0','\0','\0','\0','\0','\0','\0','\0',
    '\0','\0','\0','\0','\0','\0','\0','\0', '\0','\0','\0','\0','\0','\0','\0','\0',
    // 0x20–0x7E (printable ASCII — direct mapping)
    ' ','!','"','#','$','%','&','\'','(',')','*','+',',','-','.','/',
    '0','1','2','3','4','5','6','7','8','9',':',';','<','=','>','?',
    '@','A','B','C','D','E','F','G','H','I','J','K','L','M','N','O',
    'P','Q','R','S','T','U','V','W','X','Y','Z','[','\\',']','^','_',
    '`','a','b','c','d','e','f','g','h','i','j','k','l','m','n','o',
    'p','q','r','s','t','u','v','w','x','y','z','{','|','}','~',
    // 0x7F (undefined)
    '\0',
    // 0x80–0x9F (Windows-1252 supplemental)
    '€','\0','‚','ƒ','„','…','†','‡','ˆ','‰','Š','‹','Œ','\0','Ž','\0',
    '\0','\'','\'','"','"','•','–','—','˜','™','š','›','œ','\0','ž','Ÿ',
    // 0xA0–0xFF (Latin-1 supplement — direct Unicode identity)
    '\u{A0}','¡','¢','£','¤','¥','¦','§','¨','©','ª','«','¬','\u{AD}','®','¯',
    '°','±','²','³','´','µ','¶','·','¸','¹','º','»','¼','½','¾','¿',
    'À','Á','Â','Ã','Ä','Å','Æ','Ç','È','É','Ê','Ë','Ì','Í','Î','Ï',
    'Ð','Ñ','Ò','Ó','Ô','Õ','Ö','×','Ø','Ù','Ú','Û','Ü','Ý','Þ','ß',
    'à','á','â','ã','ä','å','æ','ç','è','é','ê','ë','ì','í','î','ï',
    'ð','ñ','ò','ó','ô','õ','ö','÷','ø','ù','ú','û','ü','ý','þ','ÿ',
];

// ─── MacRomanEncoding ───────────────────────────────────────────────────────
// ISO 32000-1 Annex D.5.
#[rustfmt::skip]
const MAC_ROMAN: [char; 256] = [
    '\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0',
    '\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0',
    ' ','!','"','#','$','%','&','\'','(',')','*','+',',','-','.','/',
    '0','1','2','3','4','5','6','7','8','9',':',';','<','=','>','?',
    '@','A','B','C','D','E','F','G','H','I','J','K','L','M','N','O',
    'P','Q','R','S','T','U','V','W','X','Y','Z','[','\\',']','^','_',
    '`','a','b','c','d','e','f','g','h','i','j','k','l','m','n','o',
    'p','q','r','s','t','u','v','w','x','y','z','{','|','}','~','\0',
    // 0x80–0xFF Mac Roman extended
    'Ä','Å','Ç','É','Ñ','Ö','Ü','á','à','â','ä','ã','å','ç','é','è',
    'ê','ë','í','ì','î','ï','ñ','ó','ò','ô','ö','õ','ú','ù','û','ü',
    '†','°','¢','£','§','•','¶','ß','®','©','™','´','¨','\u{2260}','Æ','Ø',
    '\u{221E}','±','\u{2264}','\u{2265}','¥','µ','\u{2202}','\u{2211}',
    '\u{220F}','π','\u{222B}','ª','º','\u{03A9}','æ','ø',
    '¿','¡','¬','\u{221A}','\u{0192}','\u{2248}','\u{2206}','«','»','\u{2026}',
    '\u{A0}','À','Ã','Õ','Œ','œ','\u{2013}','\u{2014}','\u{201C}','\u{201D}',
    '\u{2018}','\u{2019}','÷','\u{25CA}','ÿ','\u{0178}','\u{2044}','\u{20AC}',
    '\u{2039}','\u{203A}','\u{FB01}','\u{FB02}','\u{2021}','·','\u{201A}',
    '\u{201E}','\u{2030}','Â','Ê','Á','Ë','È','Í','Î','Ï','Ì','Ó','Ô',
    '\u{F8FF}','Ò','Ú','Û','Ù','\u{0131}','\u{02C6}','\u{02DC}','\u{00AF}',
    '\u{02D8}','\u{02D9}','\u{02DA}','\u{00B8}','\u{02DD}','\u{02DB}','\u{02C7}',
];

// ─── StandardEncoding ───────────────────────────────────────────────────────
// PostScript standard encoding per ISO 32000-1 Annex D.1.
// Groups of 16 characters per row (0x00–0xFF).
#[rustfmt::skip]
const STANDARD: [char; 256] = [
    // 0x00–0x1F
    '\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0',
    '\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0',
    // 0x20–0x2F
    ' ','!','"','#','$','%','&','\'','(',')','*','+',',','\u{2013}','.','/',
    // 0x30–0x3F
    '0','1','2','3','4','5','6','7','8','9',':',';','<','=','>','?',
    // 0x40–0x4F
    '@','A','B','C','D','E','F','G','H','I','J','K','L','M','N','O',
    // 0x50–0x5F
    'P','Q','R','S','T','U','V','W','X','Y','Z','[','\\',']','\u{02C6}','_',
    // 0x60–0x6F
    '\u{02CB}','a','b','c','d','e','f','g','h','i','j','k','l','m','n','o',
    // 0x70–0x7F
    'p','q','r','s','t','u','v','w','x','y','z','\u{2014}','|','\u{2019}','\u{02DC}','\0',
    // 0x80–0x8F
    '\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0',
    // 0x90–0x9F
    '\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0',
    // 0xA0–0xAF
    '\0','¡','¢','£','\u{2044}','¥','\u{0192}','§','\u{00A4}','\'','\"','«',
    '\u{2039}','\u{203A}','\u{FB01}','\u{FB02}',
    // 0xB0–0xBF
    '\0','\u{2013}','\u{2020}','\u{2021}','·','\0','¶','•',
    '\u{201A}','\u{201E}','\u{201C}','»','\u{2026}','\u{2030}','\0','¿',
    // 0xC0–0xCF
    '\0','`','\u{00B4}','\u{02C6}','\u{02DC}','¯','\u{02D8}','\u{02D9}',
    '¨','\0','\u{02DA}','¸','\0','\u{02DD}','\u{02DB}','\u{02C7}',
    // 0xD0–0xDF
    '\u{2014}','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0',
    // 0xE0–0xEF
    '\0','æ','\0','a','\0','\u{0131}','\0','\0','\u{0142}','ø','\u{0153}','ß',
    '\0','\0','\0','\0',
    // 0xF0–0xFF
    '\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0','\0',
];
