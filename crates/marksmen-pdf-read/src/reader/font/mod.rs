//! Font model: encoding resolution + glyph advance width decoding.
//!
//! `Font::load()` inspects the PDF font dictionary, resolves the encoding priority chain
//! (ToUnicode CMap → /Encoding name → /Encoding differences → BaseFont heuristic),
//! and loads the width table. `Font::decode()` converts raw byte strings from content
//! stream operators into `(Unicode, advance_width_pts)` pairs.

pub mod cmap;
pub mod encoding;
pub mod metrics;

use cmap::CMap;
use encoding::{EncodingKind, decode_byte, encoding_from_name};
use lopdf::{Dictionary, Document, Object};
use metrics::WidthTable;
use std::collections::HashMap;

/// A resolved PDF font: encoding + glyph widths.
#[derive(Debug, Clone)]
pub struct Font {
    /// ToUnicode CMap if present — highest priority.
    to_unicode: Option<CMap>,
    /// Named or differences-based single-byte encoding.
    encoding: EncodingKind,
    /// Per-glyph differences overriding the base encoding: byte → char.
    differences: HashMap<u8, char>,
    /// Glyph advance widths (in 1/1000 text-space units).
    pub widths: WidthTable,
    /// BaseFont name (e.g. "Arial-BoldMT").
    pub base_name: String,
    /// True when font name contains "Bold".
    pub is_bold: bool,
    /// True when font name contains "Italic" or "Oblique".
    pub is_italic: bool,
    /// True when this is a composite (Type0/CIDFont) — uses 2-byte char codes.
    is_composite: bool,
}

impl Font {
    /// Load a `Font` from a PDF font dictionary.
    pub fn load(font_dict: &Dictionary, doc: &Document) -> Self {
        let base_name = font_dict
            .get(b"BaseFont")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .unwrap_or_default();

        let subtype = font_dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .unwrap_or_default();

        let is_composite = subtype == "Type0";
        let is_bold = base_name.contains("Bold") || base_name.contains("bold");
        let is_italic = base_name.contains("Italic")
            || base_name.contains("Oblique")
            || base_name.contains("italic")
            || base_name.contains("oblique");

        // ── ToUnicode CMap ──────────────────────────────────────────────
        let to_unicode = load_to_unicode(font_dict, doc);

        // ── Encoding + differences ───────────────────────────────────────
        let (encoding, differences) = load_encoding(font_dict);

        // ── Width table ──────────────────────────────────────────────────
        let widths = if is_composite {
            load_cid_widths(font_dict, doc)
        } else {
            WidthTable::from_simple_font(font_dict, doc)
        };

        Font {
            to_unicode,
            encoding,
            differences,
            widths,
            base_name,
            is_bold,
            is_italic,
            is_composite,
        }
    }

    /// Decode a raw byte string from a PDF text operator into (char, advance_units) pairs.
    ///
    /// `advance_units` is in 1/1000 text-space units. Multiply by `font_size / 1000.0`
    /// then by the horizontal scale to get page-coordinate advance.
    pub fn decode<'a>(&'a self, bytes: &'a [u8]) -> impl Iterator<Item = (char, f32)> + 'a {
        FontDecoder {
            font: self,
            bytes,
            pos: 0,
        }
    }
}

struct FontDecoder<'a> {
    font: &'a Font,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for FontDecoder<'a> {
    type Item = (char, f32);

    fn next(&mut self) -> Option<(char, f32)> {
        if self.pos >= self.bytes.len() {
            return None;
        }

        let code: u16;
        let byte_count: usize;

        if self.font.is_composite && self.pos + 1 < self.bytes.len() {
            // Type0: 2-byte char codes.
            code = u16::from_be_bytes([self.bytes[self.pos], self.bytes[self.pos + 1]]);
            byte_count = 2;
        } else {
            code = self.bytes[self.pos] as u16;
            byte_count = 1;
        }

        self.pos += byte_count;
        let advance = self.font.widths.advance(code);

        // Priority: ToUnicode → differences → named encoding.
        let ch = if let Some(cmap) = &self.font.to_unicode {
            cmap.get(&code)
                .and_then(|s| s.chars().next())
                .unwrap_or_else(|| fallback_char(self.font, code))
        } else {
            fallback_char(self.font, code)
        };

        Some((ch, advance))
    }
}

fn fallback_char(font: &Font, code: u16) -> char {
    let byte = code as u8; // For single-byte encodings.
    if let Some(&c) = font.differences.get(&byte) {
        return c;
    }
    let c = decode_byte(font.encoding, byte);
    if c != '\0' { c } else { char::from(byte) }
}

// ─── Loading helpers ─────────────────────────────────────────────────────────

fn load_to_unicode(font_dict: &Dictionary, doc: &Document) -> Option<CMap> {
    let stream_obj = font_dict.get(b"ToUnicode").ok()?;
    let stream = match stream_obj {
        Object::Stream(s) => s,
        Object::Reference(id) => {
            if let Ok(Object::Stream(s)) = doc.get_object(*id) {
                s
            } else {
                return None;
            }
        }
        _ => return None,
    };
    let data = stream.decompressed_content().ok()?;
    let map = cmap::parse_cmap(&data);
    if map.is_empty() { None } else { Some(map) }
}

fn load_encoding(font_dict: &Dictionary) -> (EncodingKind, HashMap<u8, char>) {
    let enc_obj = match font_dict.get(b"Encoding").ok() {
        Some(o) => o,
        None => return (EncodingKind::WinAnsi, HashMap::new()),
    };

    match enc_obj {
        // Simple name: "WinAnsiEncoding" etc.
        Object::Name(n) => {
            let name = String::from_utf8_lossy(n);
            let kind = encoding_from_name(&name).unwrap_or(EncodingKind::WinAnsi);
            (kind, HashMap::new())
        }
        // Encoding dictionary with optional base name + differences.
        Object::Dictionary(d) => {
            let base_kind = d
                .get(b"BaseEncoding")
                .ok()
                .and_then(|o| o.as_name().ok())
                .and_then(|n| encoding_from_name(&String::from_utf8_lossy(n)))
                .unwrap_or(EncodingKind::WinAnsi);

            let diffs = parse_differences(d);
            (base_kind, diffs)
        }
        _ => (EncodingKind::WinAnsi, HashMap::new()),
    }
}

/// Parse the `/Differences` array: [code name name … code name …].
fn parse_differences(enc_dict: &Dictionary) -> HashMap<u8, char> {
    let mut map = HashMap::new();
    let arr = match enc_dict.get(b"Differences").ok() {
        Some(Object::Array(a)) => a,
        _ => return map,
    };

    let mut current_code: u8 = 0;
    for obj in arr {
        match obj {
            Object::Integer(i) => current_code = *i as u8,
            Object::Name(n) => {
                let glyph_name = String::from_utf8_lossy(n);
                if let Some(c) = glyph_name_to_char(&glyph_name) {
                    map.insert(current_code, c);
                }
                current_code = current_code.wrapping_add(1);
            }
            _ => {}
        }
    }
    map
}

fn load_cid_widths(font_dict: &Dictionary, doc: &Document) -> WidthTable {
    // Resolve DescendantFonts array → first CIDFont dict.
    let df_obj = match font_dict.get(b"DescendantFonts").ok() {
        Some(o) => o,
        None => return WidthTable::empty(),
    };
    let arr = match df_obj {
        Object::Array(a) => a,
        Object::Reference(id) => match doc.get_object(*id).ok() {
            Some(Object::Array(a)) => a,
            _ => return WidthTable::empty(),
        },
        _ => return WidthTable::empty(),
    };
    let cid_ref = match arr.first() {
        Some(Object::Reference(id)) => *id,
        _ => return WidthTable::empty(),
    };
    let cid_dict = match doc.get_dictionary(cid_ref).ok() {
        Some(d) => d,
        None => return WidthTable::empty(),
    };
    WidthTable::from_cid_font(cid_dict, doc)
}

/// Map PostScript glyph names from the `/Differences` array to Unicode chars.
///
/// This covers the most common glyph names; a full AGL lookup is out of scope for now.
fn glyph_name_to_char(name: &str) -> Option<char> {
    // Common AGL (Adobe Glyph List) names.
    match name {
        "space" => Some(' '),
        "exclam" => Some('!'),
        "quotedbl" => Some('"'),
        "numbersign" => Some('#'),
        "dollar" => Some('$'),
        "percent" => Some('%'),
        "ampersand" => Some('&'),
        "quotesingle" | "quoteright" => Some('\''),
        "parenleft" => Some('('),
        "parenright" => Some(')'),
        "asterisk" => Some('*'),
        "plus" => Some('+'),
        "comma" => Some(','),
        "hyphen" | "minus" => Some('-'),
        "period" => Some('.'),
        "slash" => Some('/'),
        "colon" => Some(':'),
        "semicolon" => Some(';'),
        "less" => Some('<'),
        "equal" => Some('='),
        "greater" => Some('>'),
        "question" => Some('?'),
        "at" => Some('@'),
        "bracketleft" => Some('['),
        "bracketright" => Some(']'),
        "backslash" => Some('\\'),
        "asciicircum" => Some('^'),
        "underscore" => Some('_'),
        "grave" | "quoteleft" => Some('`'),
        "braceleft" => Some('{'),
        "bar" => Some('|'),
        "braceright" => Some('}'),
        "asciitilde" => Some('~'),
        "bullet" => Some('•'),
        "endash" => Some('–'),
        "emdash" => Some('—'),
        "quotedblleft" => Some('\u{201C}'),
        "quotedblright" => Some('\u{201D}'),
        "fi" => Some('\u{FB01}'),
        "fl" => Some('\u{FB02}'),
        "ellipsis" => Some('…'),
        "dagger" => Some('†'),
        "daggerdbl" => Some('‡'),
        "trademark" => Some('™'),
        "copyright" => Some('©'),
        "registered" => Some('®'),
        "section" => Some('§'),
        "paragraph" => Some('¶'),
        "perthousand" => Some('‰'),
        "guilsinglleft" => Some('‹'),
        "guilsinglright" => Some('›'),
        "guillemotleft" => Some('«'),
        "guillemotright" => Some('»'),
        "dotaccent" => Some('˙'),
        "ring" => Some('˚'),
        "tilde" => Some('˜'),
        "Euro" => Some('€'),
        // Single-letter glyph names = the letter itself (very common).
        n if n.len() == 1 => n.chars().next(),
        // Uppercase: "A" … "Z", lowercase: "a" … "z".
        n if n.len() <= 2 => {
            // Try direct Unicode: some fonts use "uni0041" etc.
            if let Some(hex) = n.strip_prefix("uni") {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else {
                None
            }
        }
        n => {
            // "uni<4hex>" pattern.
            if let Some(hex) = n.strip_prefix("uni") {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else {
                None
            }
        }
    }
}
