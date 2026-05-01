//! RTF → Markdown extractor (RTF 1.9, full control-word dispatch).
//!
//! # Invariants
//! - Groups tracked via `Vec<CharState>` stack (push on `{`, pop on `}`).
//! - SKIP_DESTINATIONS are fully skipped (skip_depth counter).
//! - Unknown control words are silently ignored per RTF spec §1.3.
//! - `\uN?` decoded; `\'HH` decoded via Win-1252; `\bin N` bytes skipped.
//! - `\outlinelevel N` maps to heading level N+1.
//! - `\cellx N` values collected per `\trowd` to compute table column count.
//! - Bold + font-size ≥ 32 heuristic used as heading fallback when no \sN style set.
//! - `\deleted` and related revision-tracking words suppress text emission.

use anyhow::Result;

pub fn parse_rtf(bytes: &[u8]) -> Result<String> {
    let chars: Vec<char> = if let Ok(s) = std::str::from_utf8(bytes) {
        s.chars().collect()
    } else {
        bytes
            .iter()
            .map(|&b| {
                if b < 0x80 {
                    char::from(b)
                } else {
                    crate::codepage::WIN1252[b as usize - 0x80]
                }
            })
            .collect()
    };
    let mut p = RtfParser::default();
    p.process(&chars);
    Ok(p.finish())
}

// ── Skipped destinations ──────────────────────────────────────────────────────

const SKIP: &[&str] = &[
    "fonttbl",
    "colortbl",
    "stylesheet",
    "listtable",
    "listoverridetable",
    "info",
    "pict",
    "objdata",
    "datafield",
    "themedata",
    "colorschememapping",
    "rsidtbl",
    "generator",
    "pgdscphs",
    "pgptbl",
    "wgrffmtfilter",
    "docvar",
    "ftnsep",
    "ftnsepc",
    "aftnsep",
    "aftnsepc",
    "bkmkstart",
    "bkmkend",
    "xmlnstbl",
    "expandedcolortbl",
    "momath",
];

// ── Character / paragraph state ───────────────────────────────────────────────

#[derive(Clone, Default, Debug)]
struct CharState {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    deleted: bool, // inside \deleted revision
    mono: bool,
    font_size: u32, // half-points; 0 = unset
    heading: u8,    // 0 = body paragraph
    outline: i32,   // from \outlinelevelN; -1 = none
    in_list: bool,
    ordered: bool,
}

impl CharState {
    fn new() -> Self {
        Self {
            font_size: 24,
            outline: -1,
            ..Default::default()
        }
    }
}

// ── Parser ────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct RtfParser {
    stack: Vec<CharState>,
    cur: CharState,
    run: String,
    out: String,
    skip_depth: u32,
    // List tracking
    list_depth: u32,
    item_counters: Vec<u32>,
    // Table tracking
    in_table: bool,
    row_cells: Vec<String>,
    first_row: bool,
    cellx_vals: Vec<u32>, // \cellx values collected per trowd
    // Hyperlink fields
    in_fldinst: bool,
    pending_url: String,
    in_fldrslt: bool,
}

impl RtfParser {
    fn process(&mut self, chars: &[char]) {
        self.cur = CharState::new();
        let len = chars.len();
        let mut i = 0;
        while i < len {
            match chars[i] {
                '{' => {
                    self.stack.push(self.cur.clone());
                    if self.skip_depth > 0 {
                        self.skip_depth += 1;
                    }
                    i += 1;
                }
                '}' => {
                    self.flush_run();
                    if self.skip_depth > 0 {
                        self.skip_depth -= 1;
                    }
                    if let Some(prev) = self.stack.pop() {
                        self.cur = prev;
                    }
                    if self.in_fldinst {
                        self.in_fldinst = false;
                    }
                    if self.in_fldrslt {
                        self.in_fldrslt = false;
                        self.pending_url.clear();
                    }
                    i += 1;
                }
                '\\' => {
                    i += 1;
                    if i >= len {
                        break;
                    }
                    match chars[i] {
                        '\n' | '\r' => {
                            i += 1;
                        }
                        '{' | '}' | '\\' => {
                            if self.skip_depth == 0 && !self.cur.deleted {
                                self.run.push(chars[i]);
                            }
                            i += 1;
                        }
                        '\'' => {
                            i += 1;
                            if i + 1 < len {
                                let hex = format!("{}{}", chars[i], chars[i + 1]);
                                i += 2;
                                if let Ok(b) = u8::from_str_radix(&hex, 16) {
                                    if self.skip_depth == 0 && !self.cur.deleted {
                                        let ch = if b < 0x80 {
                                            char::from(b)
                                        } else {
                                            crate::codepage::WIN1252[b as usize - 0x80]
                                        };
                                        self.run.push(ch);
                                    }
                                }
                            }
                        }
                        '-' => {
                            if self.skip_depth == 0 {
                                self.run.push('\u{00AD}');
                            }
                            i += 1;
                        }
                        '~' => {
                            if self.skip_depth == 0 {
                                self.run.push('\u{00A0}');
                            }
                            i += 1;
                        }
                        '*' => {
                            // {\*\destination}
                            i += 1;
                            while i < len && chars[i] == ' ' {
                                i += 1;
                            }
                            if i < len && chars[i] == '\\' {
                                i += 1;
                                let (word, _, n) = read_ctrl(chars, i);
                                i += n;
                                if SKIP.contains(&word.as_str()) {
                                    self.skip_depth += 1;
                                }
                            }
                        }
                        'b' if i + 2 < len && chars[i + 1] == 'i' && chars[i + 2] == 'n' => {
                            // \binN — skip N raw bytes
                            i += 3;
                            let (_, param, n) = read_ctrl(chars, i);
                            i += n;
                            let skip = param.unwrap_or(0).max(0) as usize;
                            i += skip.min(len - i);
                        }
                        _ => {
                            let (word, param, n) = read_ctrl(chars, i);
                            i += n;
                            if SKIP.contains(&word.as_str()) {
                                self.skip_depth += 1;
                            } else if self.skip_depth == 0 {
                                self.dispatch(&word, param, chars, &mut i);
                            }
                        }
                    }
                }
                '\n' | '\r' => {
                    i += 1;
                }
                c => {
                    if self.skip_depth == 0 && !self.cur.deleted {
                        self.run.push(c);
                    }
                    i += 1;
                }
            }
        }
        self.flush_run();
    }

    fn dispatch(&mut self, word: &str, param: Option<i32>, _chars: &[char], _i: &mut usize) {
        match word {
            // ── Paragraph resets ──────────────────────────────────────────────
            "pard" => {
                self.flush_run();
                let mono = self.cur.mono;
                self.cur = CharState::new();
                self.cur.mono = mono;
                self.in_table = false; // \pard resets intbl; re-set by \intbl
            }
            "par" | "sect" | "page" | "column" => {
                self.flush_run();
                self.emit_par();
            }
            "line" => {
                self.flush_run();
                self.out.push_str("  \n");
            }
            "tab" => {
                self.run.push_str("    ");
            }
            "ltrpar" | "rtlpar" => {} // layout hints, ignored

            // ── Character formatting ─────────────────────────────────────────
            "b" => {
                self.flush_run();
                self.cur.bold = param.map_or(true, |p| p != 0);
            }
            "i" => {
                self.flush_run();
                self.cur.italic = param.map_or(true, |p| p != 0);
            }
            "ul" => {
                self.flush_run();
                self.cur.underline = true;
            }
            "uld" => {
                self.flush_run();
                self.cur.underline = true;
            }
            "ulw" => {
                self.flush_run();
                self.cur.underline = true;
            }
            "ulnone" | "uldb" if param == Some(0) => {
                self.flush_run();
                self.cur.underline = false;
            }
            "ulnone" => {
                self.flush_run();
                self.cur.underline = false;
            }
            "strike" => {
                self.flush_run();
                self.cur.strike = param.map_or(true, |p| p != 0);
            }
            "striked" => {
                self.flush_run();
                self.cur.strike = param.map_or(true, |p| p != 0);
            }
            "fs" => {
                self.flush_run();
                self.cur.font_size = param.unwrap_or(24).max(0) as u32;
            }
            "f" => {
                self.flush_run();
                self.cur.mono = param.unwrap_or(0) == 1;
            }
            "plain" => {
                self.flush_run();
                let sz = self.cur.font_size;
                self.cur = CharState::new();
                self.cur.font_size = sz;
            }
            "nosupersub" | "up" | "dn" | "sub" | "super" => {} // super/sub: passthrough text
            "caps" | "scaps" | "shad" | "outl" => {}           // decoration hints, ignored
            "v" => {
                self.cur.deleted = true;
            } // hidden text
            // Revision tracking — skip deleted text
            "deleted" => {
                self.flush_run();
                self.cur.deleted = param.map_or(true, |p| p != 0);
            }
            "insrsid" | "revtbl" | "rsid" | "charrsid" | "pararesid" => {}

            // ── Headings via stylesheet ──────────────────────────────────────
            "s1" => {
                self.flush_run();
                self.cur.heading = 1;
                self.cur.bold = true;
            }
            "s2" => {
                self.flush_run();
                self.cur.heading = 2;
                self.cur.bold = true;
            }
            "s3" => {
                self.flush_run();
                self.cur.heading = 3;
                self.cur.bold = true;
            }
            "s4" => {
                self.flush_run();
                self.cur.heading = 4;
                self.cur.bold = true;
            }
            "s5" => {
                self.flush_run();
                self.cur.heading = 5;
                self.cur.bold = true;
            }
            "s6" => {
                self.flush_run();
                self.cur.heading = 6;
            }
            // Heading via \outlinelevel (0-based)
            "outlinelevel" => {
                self.flush_run();
                let lvl = param.unwrap_or(-1);
                self.cur.outline = lvl;
                if lvl >= 0 && lvl <= 5 {
                    self.cur.heading = (lvl + 1) as u8;
                    self.cur.bold = true;
                }
            }

            // ── Lists ──────────────────────────────────────────────────────────
            "ls" => {
                self.flush_run();
                self.cur.in_list = true;
                self.list_depth += 1;
                if self.item_counters.len() < self.list_depth as usize {
                    self.item_counters.push(0);
                }
            }
            "pnlvlblt" => {
                self.flush_run();
                self.cur.in_list = true;
                self.cur.ordered = false;
                self.list_depth = self.list_depth.saturating_add(1);
            }
            "pnlvlbody" => {
                self.flush_run();
                self.cur.in_list = true;
                self.cur.ordered = true;
                self.list_depth += 1;
                if self.item_counters.len() < self.list_depth as usize {
                    self.item_counters.push(0);
                }
            }
            "listid" | "listoverridecount" | "listoverride" | "ilvl" => {}
            "levelfollow" | "levelstartat" | "levelspace" | "levelindent" | "levelnfc"
            | "levelnfcn" => {}

            // ── Tables ──────────────────────────────────────────────────────────
            "trowd" => {
                self.flush_run();
                self.in_table = true;
                self.row_cells.clear();
                self.cellx_vals.clear();
            }
            "intbl" => {
                self.in_table = true;
            }
            "cellx" => {
                if let Some(n) = param {
                    self.cellx_vals.push(n.max(0) as u32);
                }
            }
            "cell" => {
                let c = std::mem::take(&mut self.run);
                self.row_cells.push(c.trim().to_string());
            }
            "row" => {
                self.flush_table_row();
            }
            "trgaph" | "trleft" | "trrh" | "trpaddl" | "trpaddr" | "trpaddb" | "trpaddt" => {}
            "clbrdrt" | "clbrdrl" | "clbrdrb" | "clbrdrr" | "clcbpat" | "clshdng" => {}
            "brdrs" | "brdrw" | "brdrcf" | "brdrnil" => {}

            // ── Fields (hyperlinks) ─────────────────────────────────────────────
            "fldinst" => {
                self.flush_run();
                self.in_fldinst = true;
            }
            "fldrslt" => {
                self.flush_run();
                self.in_fldinst = false;
                self.in_fldrslt = true;
            }
            "field" => {}

            // ── Unicode ─────────────────────────────────────────────────────────
            "u" => {
                if let Some(n) = param {
                    let cp = if n < 0 { (n + 65536) as u32 } else { n as u32 };
                    if let Some(ch) = char::from_u32(cp) {
                        if !self.cur.deleted {
                            self.run.push(ch);
                        }
                    }
                }
            }
            "bullet" => {
                self.run.push('\u{2022}');
            }
            "endash" => {
                self.run.push('\u{2013}');
            }
            "emdash" => {
                self.run.push('\u{2014}');
            }
            "lquote" | "rquote" => {
                self.run.push('\'');
            }
            "ldblquote" | "rdblquote" => {
                self.run.push('"');
            }

            // ── Paragraph alignment / spacing — ignored structurally ────────────
            "ql" | "qr" | "qc" | "qj" | "li" | "ri" | "fi" | "sb" | "sa" | "sl" | "slmult"
            | "widctlpar" | "nowidctlpar" | "adjustright" | "keepn" | "keep" | "noline"
            | "lang" | "langnp" | "langfe" | "ltrch" | "rtlch" | "cf" | "highlight" | "expnd"
            | "kerning" => {}

            // ── Section ─────────────────────────────────────────────────────────
            "sectd" | "pgwsxn" | "pghsxn" | "marglsxn" | "margrsxn" | "headery" | "footery"
            | "pgndec" | "pgnrestart" => {}

            _ => {} // Unknown control words silently ignored (RTF spec §1.3)
        }
    }

    fn flush_run(&mut self) {
        if self.run.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.run);
        if self.cur.deleted {
            return;
        }

        if self.in_fldinst {
            if let Some(start) = text.find("HYPERLINK") {
                let rest = text[start + 9..].trim();
                let url = rest.trim_matches('"').trim_matches('\'').trim();
                self.pending_url = url.to_string();
            }
            return;
        }

        if self.in_fldrslt && !self.pending_url.is_empty() {
            self.out.push('[');
            self.append_formatted(&text);
            self.out.push_str("](");
            self.out.push_str(&self.pending_url.clone());
            self.out.push(')');
            return;
        }

        self.append_formatted(&text);
    }

    fn append_formatted(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let b = self.cur.bold;
        let i = self.cur.italic;
        let s = self.cur.strike;
        let u = self.cur.underline;
        let m = self.cur.mono;

        if s {
            self.out.push_str("~~");
        }
        if u {
            self.out.push_str("<u>");
        }
        if m {
            self.out.push('`');
        } else if b && i {
            self.out.push_str("***");
        } else if b {
            self.out.push_str("**");
        } else if i {
            self.out.push('*');
        }

        self.out.push_str(text);

        if m {
            self.out.push('`');
        } else if b && i {
            self.out.push_str("***");
        } else if b {
            self.out.push_str("**");
        } else if i {
            self.out.push('*');
        }
        if u {
            self.out.push_str("</u>");
        }
        if s {
            self.out.push_str("~~");
        }
    }

    fn emit_par(&mut self) {
        if self.in_table {
            return;
        }

        // Heading detection: explicit style > outlinelevel > font-size heuristic
        let heading = if self.cur.heading > 0 {
            self.cur.heading
        } else if self.cur.outline >= 0 && self.cur.outline <= 5 {
            (self.cur.outline + 1) as u8
        } else if self.cur.bold && self.cur.font_size >= 32 {
            // Font-size heuristic bucket
            match self.cur.font_size {
                sz if sz >= 44 => 1,
                sz if sz >= 36 => 2,
                sz if sz >= 30 => 3,
                _ => 4,
            }
        } else {
            0
        };

        let content = extract_last_paragraph(&mut self.out);
        let content = content.trim().to_string();
        if content.is_empty() {
            self.out.push_str("\n\n");
            return;
        }

        if heading > 0 {
            self.out
                .push_str(&format!("{} {}\n\n", "#".repeat(heading as usize), content));
            self.cur.heading = 0;
        } else if self.cur.in_list {
            let depth = self.list_depth.saturating_sub(1);
            let indent = "  ".repeat(depth as usize);
            if self.cur.ordered {
                let n = self
                    .item_counters
                    .last_mut()
                    .map(|c| {
                        *c += 1;
                        *c
                    })
                    .unwrap_or(1);
                self.out
                    .push_str(&format!("{}{}. {}\n", indent, n, content));
            } else {
                self.out.push_str(&format!("{}- {}\n", indent, content));
            }
        } else {
            self.out.push_str(&format!("{}\n\n", content));
        }
    }

    fn flush_table_row(&mut self) {
        if self.row_cells.is_empty() {
            return;
        }
        // Cell content from run buffer if cell was not explicitly closed
        if !self.run.is_empty() {
            let c = std::mem::take(&mut self.run);
            self.row_cells.push(c.trim().to_string());
        }
        let col_count = self.cellx_vals.len().max(self.row_cells.len()).max(1);
        let row = format!("| {} |", self.row_cells.join(" | "));
        self.out.push_str(&row);
        self.out.push('\n');
        if self.first_row {
            let sep = format!("| {} |", vec!["---"; col_count].join(" | "));
            self.out.push_str(&sep);
            self.out.push('\n');
            self.first_row = false;
        }
        self.row_cells.clear();
    }

    fn finish(mut self) -> String {
        // Flush any pending table
        if self.in_table {
            self.flush_table_row();
        }
        let mut out = self.out;
        while out.contains("\n\n\n") {
            out = out.replace("\n\n\n", "\n\n");
        }
        out.trim().to_string()
    }
}

/// Extract the content since the last `\n\n` boundary, removing it from `s`.
fn extract_last_paragraph(s: &mut String) -> String {
    // Find the last double-newline
    if let Some(pos) = s.rfind("\n\n") {
        let tail = s[pos + 2..].to_string();
        s.truncate(pos + 2);
        tail
    } else if let Some(pos) = s.rfind('\n') {
        let tail = s[pos + 1..].to_string();
        s.truncate(pos + 1);
        tail
    } else {
        let all = s.clone();
        s.clear();
        all
    }
}

/// Read a control word starting at `chars[start]`.
/// Returns `(word, param, consumed_char_count)`.
pub fn read_ctrl(chars: &[char], start: usize) -> (String, Option<i32>, usize) {
    let mut i = start;
    let len = chars.len();
    let mut word = String::new();
    while i < len && chars[i].is_ascii_alphabetic() {
        word.push(chars[i]);
        i += 1;
    }
    let mut param: Option<i32> = None;
    if i < len && (chars[i] == '-' || chars[i].is_ascii_digit()) {
        let neg = chars[i] == '-';
        if neg {
            i += 1;
        }
        let mut num = String::new();
        while i < len && chars[i].is_ascii_digit() {
            num.push(chars[i]);
            i += 1;
        }
        if let Ok(n) = num.parse::<i32>() {
            param = Some(if neg { -n } else { n });
        }
    }
    if i < len && chars[i] == ' ' {
        i += 1;
    }
    (word, param, i - start)
}
