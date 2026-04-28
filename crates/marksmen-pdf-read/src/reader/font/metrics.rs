//! Glyph advance width resolution from PDF font dictionaries.
//!
//! Implements §9.7.4 of ISO 32000-1 for simple fonts (Type1, TrueType, Type3)
//! and §9.7.4.3 for composite (Type0/CID) fonts.
//!
//! All widths in the PDF font dictionary are in 1/1000 text-space units.
//! To convert to points: `(width_units / 1000.0) * font_size_pts`.

use lopdf::{Dictionary, Document, Object};

/// Width lookup table for a font, resolved from `/Widths` or `/W` arrays.
#[derive(Debug, Clone)]
pub struct WidthTable {
    /// For simple fonts: (first_char, widths[])
    kind: WidthKind,
    /// Default width (typically 1000 for simple fonts, may differ for CIDFonts).
    pub default_width: f32,
}

#[derive(Debug, Clone)]
enum WidthKind {
    Simple { first_char: u32, widths: Vec<f32> },
    Cid(Vec<CidWidthEntry>),
    Empty,
}

/// One entry in a CIDFont `/W` array.
#[derive(Debug, Clone)]
enum CidWidthEntry {
    /// Individual widths: CIDs `start..start+widths.len()`.
    Span { start: u32, widths: Vec<f32> },
    /// Uniform width for CID range `[start, end]`.
    Range { start: u32, end: u32, width: f32 },
}

impl WidthTable {
    /// Resolve the advance width (in 1/1000 text units) for `char_code`.
    pub fn advance(&self, char_code: u16) -> f32 {
        match &self.kind {
            WidthKind::Empty => self.default_width,
            WidthKind::Simple { first_char, widths } => {
                let idx = char_code as u32;
                if idx >= *first_char {
                    let offset = (idx - first_char) as usize;
                    widths.get(offset).copied().unwrap_or(self.default_width)
                } else {
                    self.default_width
                }
            }
            WidthKind::Cid(entries) => {
                let cid = char_code as u32;
                for entry in entries {
                    match entry {
                        CidWidthEntry::Span { start, widths } => {
                            if cid >= *start {
                                let off = (cid - start) as usize;
                                if off < widths.len() {
                                    return widths[off];
                                }
                            }
                        }
                        CidWidthEntry::Range { start, end, width } => {
                            if cid >= *start && cid <= *end {
                                return *width;
                            }
                        }
                    }
                }
                self.default_width
            }
        }
    }

    /// Build a `WidthTable` from a simple font dictionary.
    pub fn from_simple_font(font_dict: &Dictionary, doc: &Document) -> Self {
        let first_char = font_dict
            .get(b"FirstChar")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0) as u32;

        let default_width = descriptor_missing_width(font_dict, doc).unwrap_or(1000.0);

        let widths = if let Ok(w_obj) = font_dict.get(b"Widths") {
            let arr = match w_obj {
                Object::Array(a) => Some(a),
                Object::Reference(id) => doc.get_object(*id).ok().and_then(|o| {
                    if let Object::Array(a) = o {
                        Some(a)
                    } else {
                        None
                    }
                }),
                _ => None,
            };
            arr.map(|a| a.iter().map(obj_to_f32).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        WidthTable {
            kind: WidthKind::Simple { first_char, widths },
            default_width,
        }
    }

    /// Build a `WidthTable` from a CIDFont (DescendantFonts entry).
    pub fn from_cid_font(cid_dict: &Dictionary, doc: &Document) -> Self {
        let dw = obj_to_f32(cid_dict.get(b"DW").unwrap_or(&Object::Integer(1000)));

        let w_obj = match cid_dict.get(b"W").ok() {
            Some(o) => match o {
                Object::Array(a) => Some(a.clone()),
                Object::Reference(id) => doc.get_object(*id).ok().and_then(|o| {
                    if let Object::Array(a) = o {
                        Some(a.clone())
                    } else {
                        None
                    }
                }),
                _ => None,
            },
            None => None,
        };

        let Some(w_arr) = w_obj else {
            return WidthTable {
                kind: WidthKind::Empty,
                default_width: dw,
            };
        };

        let mut entries: Vec<CidWidthEntry> = Vec::new();
        let mut i = 0;
        while i < w_arr.len() {
            let c1 = match obj_to_u32(&w_arr[i]) {
                Some(v) => v,
                None => {
                    i += 1;
                    continue;
                }
            };
            i += 1;
            if i >= w_arr.len() {
                break;
            }

            match &w_arr[i] {
                Object::Array(sub) => {
                    let widths: Vec<f32> = sub.iter().map(obj_to_f32).collect();
                    entries.push(CidWidthEntry::Span { start: c1, widths });
                    i += 1;
                }
                other => {
                    // Next element should be c2 (end of range) followed by width.
                    let c2 = obj_to_u32(other).unwrap_or(c1);
                    i += 1;
                    let w = w_arr.get(i).map(obj_to_f32).unwrap_or(dw);
                    i += 1;
                    entries.push(CidWidthEntry::Range {
                        start: c1,
                        end: c2,
                        width: w,
                    });
                }
            }
        }

        WidthTable {
            kind: WidthKind::Cid(entries),
            default_width: dw,
        }
    }

    pub fn empty() -> Self {
        WidthTable {
            kind: WidthKind::Empty,
            default_width: 1000.0,
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn obj_to_f32(o: &Object) -> f32 {
    match o {
        Object::Real(f) => *f as f32,
        Object::Integer(i) => *i as f32,
        _ => 0.0,
    }
}

fn obj_to_u32(o: &Object) -> Option<u32> {
    match o {
        Object::Integer(i) => Some(*i as u32),
        Object::Real(f) => Some(*f as u32),
        _ => None,
    }
}

/// Read `MissingWidth` from the `/FontDescriptor` sub-dictionary.
fn descriptor_missing_width(font_dict: &Dictionary, doc: &Document) -> Option<f32> {
    let fd_obj = font_dict.get(b"FontDescriptor").ok()?;
    let fd = match fd_obj {
        Object::Dictionary(d) => d,
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        _ => return None,
    };
    fd.get(b"MissingWidth").ok().and_then(|o| match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(f) => Some(*f as f32),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_font_advance_in_range() {
        let table = WidthTable {
            kind: WidthKind::Simple {
                first_char: 65,
                widths: vec![722.0, 667.0, 667.0],
            },
            default_width: 500.0,
        };
        assert_eq!(table.advance(65), 722.0); // A
        assert_eq!(table.advance(66), 667.0); // B
        assert_eq!(table.advance(68), 500.0); // D — beyond widths, use default
    }

    #[test]
    fn cid_range_advance() {
        let table = WidthTable {
            kind: WidthKind::Cid(vec![CidWidthEntry::Range {
                start: 0,
                end: 255,
                width: 500.0,
            }]),
            default_width: 1000.0,
        };
        assert_eq!(table.advance(100), 500.0);
        assert_eq!(table.advance(300), 1000.0); // outside range
    }
}
