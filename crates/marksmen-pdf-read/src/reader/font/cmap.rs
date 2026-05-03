//! ToUnicode CMap stream parser.
//!
//! A ToUnicode CMap maps char codes (1 or 2 bytes) to Unicode strings.
//! Format is a subset of PostScript CMap syntax — see ISO 32000-1 §9.10.3.
//!
//! Relevant grammar productions:
//! ```text
//! beginbfchar
//!   <src-code-hex> <dst-unicode-hex>
//! endbfchar
//!
//! beginbfrange
//!   <start-hex> <end-hex> <dst-start-hex>      -- sequential mapping
//!   <start-hex> <end-hex> [<hex1> <hex2> …]    -- explicit array mapping
//! endbfrange
//! ```

use std::collections::HashMap;

/// Map from single char code (up to 2 bytes, stored as u16) to a Unicode String.
///
/// Single-byte encodings use the low byte; two-byte CIDFont CMaps use the full u16.
pub type CMap = HashMap<u16, String>;

/// Parse a ToUnicode CMap stream into a char-code → Unicode mapping.
///
/// `data` is the raw decompressed stream bytes. Leading/trailing whitespace and
/// PostScript comments (lines starting with `%`) are ignored.
pub fn parse_cmap(data: &[u8]) -> CMap {
    let text = String::from_utf8_lossy(data);
    let mut map = CMap::new();

    let mut iter = tokens(&text);

    while let Some(tok) = iter.next() {
        match tok.as_str() {
            "beginbfchar" => parse_bfchar(&mut iter, &mut map),
            "beginbfrange" => parse_bfrange(&mut iter, &mut map),
            _ => {}
        }
    }

    map
}

// ─── Section parsers ─────────────────────────────────────────────────────────

fn parse_bfchar(iter: &mut impl Iterator<Item = String>, map: &mut CMap) {
    loop {
        let src = match iter.next() {
            Some(t) if t == "endbfchar" => return,
            Some(t) => t,
            None => return,
        };
        let dst = match iter.next() {
            Some(t) => t,
            None => return,
        };
        if let (Some(code), Some(uni)) = (parse_hex_code(&src), parse_hex_string(&dst)) {
            map.insert(code, uni);
        }
    }
}

fn parse_bfrange(iter: &mut impl Iterator<Item = String>, map: &mut CMap) {
    loop {
        let start_tok = match iter.next() {
            Some(t) if t == "endbfrange" => return,
            Some(t) => t,
            None => return,
        };
        let end_tok = match iter.next() {
            Some(t) => t,
            None => return,
        };
        let dst_tok = match iter.next() {
            Some(t) => t,
            None => return,
        };

        let (start, end) = match (parse_hex_code(&start_tok), parse_hex_code(&end_tok)) {
            (Some(s), Some(e)) => (s, e),
            _ => continue,
        };

        if dst_tok == "[" {
            // Explicit array: one Unicode string per code in [start, end].
            let mut code = start;
            loop {
                let elem = match iter.next() {
                    Some(t) => t,
                    None => break,
                };
                if elem == "]" {
                    break;
                }
                if let Some(uni) = parse_hex_string(&elem) {
                    map.insert(code, uni);
                }
                code = code.saturating_add(1);
                if code > end {
                    break;
                }
            }
        } else if let Some(base_uni) = parse_hex_string(&dst_tok) {
            // Sequential: base_uni + (code - start) for each code.
            // base_uni is a Unicode string; we increment the *last* code point.
            let base_cp = base_uni.chars().last().map(|c| c as u32).unwrap_or(0);
            for offset in 0..=(end.wrapping_sub(start)) {
                let code = start + offset;
                let cp = base_cp + offset as u32;
                // Rebuild string: all prefix chars + incremented last char.
                let mut chars: Vec<char> = base_uni.chars().collect();
                if let Some(last) = chars.last_mut()
                    && let Some(c) = char::from_u32(cp) {
                        *last = c;
                    }
                map.insert(code, chars.into_iter().collect());
            }
        }
    }
}

// ─── Tokenizer ──────────────────────────────────────────────────────────────

/// Tokenize the CMap text into PostScript-like tokens, skipping comments.
fn tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    // Simpler: collect all tokens via split_whitespace-like logic that respects
    // hex strings <...> and arrays [...].
    tokenize_cmap(text).into_iter()
}

fn tokenize_cmap(text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, c)) = chars.next() {
        match c {
            // Comment: skip to end of line.
            '%' => {
                for (_, ch) in chars.by_ref() {
                    if ch == '\n' {
                        break;
                    }
                }
            }
            // Hex string.
            '<' => {
                let mut hex = String::from("<");
                for (_, ch) in chars.by_ref() {
                    hex.push(ch);
                    if ch == '>' {
                        break;
                    }
                }
                tokens.push(hex);
            }
            // Array brackets.
            '[' => tokens.push("[".to_string()),
            ']' => tokens.push("]".to_string()),
            // Whitespace: skip.
            c if c.is_whitespace() => {}
            // Regular token.
            c => {
                let mut tok = String::new();
                tok.push(c);
                while let Some(&(_, next)) = chars.peek() {
                    if next.is_whitespace()
                        || next == '<'
                        || next == '>'
                        || next == '['
                        || next == ']'
                    {
                        break;
                    }
                    tok.push(next);
                    chars.next();
                }
                tokens.push(tok);
            }
        }
    }

    tokens
}

// ─── Hex parsing helpers ─────────────────────────────────────────────────────

/// Parse a `<XXXX>` hex string into a u16 char code (1–2 bytes per PDF CMap).
fn parse_hex_code(tok: &str) -> Option<u16> {
    let inner = tok.strip_prefix('<')?.strip_suffix('>')?;
    let inner = inner.trim();
    u16::from_str_radix(inner, 16).ok()
}

/// Parse a `<XXXX>` hex string into a Unicode `String`.
/// The hex bytes are interpreted as UTF-16BE.
fn parse_hex_string(tok: &str) -> Option<String> {
    let inner = tok.strip_prefix('<')?.strip_suffix('>')?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Some(String::new());
    }

    // Must be even number of hex digits.
    if inner.len() % 2 != 0 {
        return None;
    }

    let bytes: Option<Vec<u8>> = (0..inner.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&inner[i..i + 2], 16).ok())
        .collect();
    let bytes = bytes?;

    // Interpret as UTF-16BE code units.
    if bytes.len() % 2 == 0 {
        let utf16: Vec<u16> = bytes
            .chunks(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect();
        String::from_utf16(&utf16).ok()
    } else {
        // Single-byte fallback: treat as Latin-1.
        Some(bytes.iter().map(|&b| char::from(b)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bfchar_single_byte() {
        let cmap = b"beginbfchar\n<41> <0041>\nendbfchar\n";
        let map = parse_cmap(cmap);
        assert_eq!(map.get(&0x41), Some(&"A".to_string()));
    }

    #[test]
    fn bfchar_two_byte_code() {
        let cmap = b"beginbfchar\n<0041> <0041>\nendbfchar\n";
        let map = parse_cmap(cmap);
        assert_eq!(map.get(&0x0041), Some(&"A".to_string()));
    }

    #[test]
    fn bfrange_sequential() {
        let cmap = b"beginbfrange\n<41> <43> <0041>\nendbfrange\n";
        let map = parse_cmap(cmap);
        assert_eq!(map.get(&0x41), Some(&"A".to_string()));
        assert_eq!(map.get(&0x42), Some(&"B".to_string()));
        assert_eq!(map.get(&0x43), Some(&"C".to_string()));
    }

    #[test]
    fn bfrange_array() {
        let cmap = b"beginbfrange\n<41> <43> [<0041> <0042> <0043>]\nendbfrange\n";
        let map = parse_cmap(cmap);
        assert_eq!(map.get(&0x41), Some(&"A".to_string()));
        assert_eq!(map.get(&0x43), Some(&"C".to_string()));
    }

    #[test]
    fn comment_skipped() {
        let cmap = b"% comment\nbeginbfchar\n<41> <0041>\nendbfchar\n";
        let map = parse_cmap(cmap);
        assert_eq!(map.get(&0x41), Some(&"A".to_string()));
    }
}
