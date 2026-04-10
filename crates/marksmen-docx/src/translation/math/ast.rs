use quick_xml::events::{BytesText, Event};
use quick_xml::Writer;
use std::io::Cursor;

/// Generates raw OMML Strings from LaTeX macros mapped into `<m:oMath>` namespace blocks.
pub struct OmmlAstGenerator;

impl OmmlAstGenerator {
    /// Takes a full latex string and generates OMML
    pub fn translate(latex: &str) -> String {
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        
        // Root container for a math equation group inner block
        // Normally placed in m:oMathPara for display and m:oMath for inline
        Self::write_math_run(&mut writer, latex);
        
        let result = writer.into_inner().into_inner();
        String::from_utf8(result).unwrap_or_default()
    }

    /// Recursively evaluate mathematical layout into runs.
    fn write_math_run<W: std::io::Write>(writer: &mut Writer<W>, latex: &str) {
        // We will tokenize the latex string into meaningful chunks: commands (\frac), groups ({}), and raw text run identifiers.
        
        let chars: Vec<char> = latex.chars().collect();
        let mut i = 0;
        
        while i < chars.len() {
            let ch = chars[i];
            
            if ch == '\\' {
                // LaTeX command
                i += 1;
                let mut cmd = String::new();
                while i < chars.len() && chars[i].is_alphabetic() {
                    cmd.push(chars[i]);
                    i += 1;
                }
                
                match cmd.as_str() {
                    "frac" => {
                        // Fraction <m:f>
                        // Expect two groups: {numerator}{denominator}
                        let (num, new_i) = Self::extract_group(&chars, i);
                        let (den, new_i2) = Self::extract_group(&chars, new_i);
                        i = new_i2;
                        
                        writer.write_event(Event::Start(quick_xml::events::BytesStart::new("m:f"))).unwrap();
                        
                        // Numerator <m:num>
                        writer.write_event(Event::Start(quick_xml::events::BytesStart::new("m:num"))).unwrap();
                        Self::write_math_run(writer, &num);
                        writer.write_event(Event::End(quick_xml::events::BytesEnd::new("m:num"))).unwrap();
                        
                        // Denominator <m:den>
                        writer.write_event(Event::Start(quick_xml::events::BytesStart::new("m:den"))).unwrap();
                        Self::write_math_run(writer, &den);
                        writer.write_event(Event::End(quick_xml::events::BytesEnd::new("m:den"))).unwrap();
                        
                        writer.write_event(Event::End(quick_xml::events::BytesEnd::new("m:f"))).unwrap();
                    }
                    _ => {
                        // Unrecognized macro, emit as raw text for now
                        let text = format!("\\{}", cmd);
                        Self::write_text_run(writer, &text);
                    }
                }
                continue; // Skip the manual increment since the cmd loop advanced `i`
            } else if ch == '{' || ch == '}' || ch.is_whitespace() {
                // Ignore raw grouping characters outside macro resolution
                i += 1;
            } else {
                // Character identifier
                let mut text = String::new();
                while i < chars.len() && chars[i] != '\\' && chars[i] != '{' && chars[i] != '}' && !chars[i].is_whitespace() {
                    text.push(chars[i]);
                    i += 1;
                }
                Self::write_text_run(writer, &text);
            }
        }
    }

    /// Extracts text within {...} handling nested groups. 
    /// Returns the inner string and the new index.
    fn extract_group(chars: &[char], start_idx: usize) -> (String, usize) {
        let mut i = start_idx;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        
        if i >= chars.len() {
            return (String::new(), i);
        }
        
        if chars[i] == '{' {
            let mut depth = 1;
            let mut content = String::new();
            i += 1;
            
            while i < chars.len() && depth > 0 {
                if chars[i] == '{' {
                    depth += 1;
                } else if chars[i] == '}' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1; // Consume closing brace
                        break;
                    }
                }
                content.push(chars[i]);
                i += 1;
            }
            (content, i)
        } else {
            // Single character group (e.g. \frac 1 2)
            let content = chars[i].to_string();
            (content, i + 1)
        }
    }

    /// Emits a standard `<m:r><m:t>text</m:t></m:r>` structure.
    fn write_text_run<W: std::io::Write>(writer: &mut Writer<W>, text: &str) {
        let t = BytesText::new(text);
        writer.write_event(Event::Start(quick_xml::events::BytesStart::new("m:r"))).unwrap();
        writer.write_event(Event::Start(quick_xml::events::BytesStart::new("m:t"))).unwrap();
        writer.write_event(Event::Text(t)).unwrap();
        writer.write_event(Event::End(quick_xml::events::BytesEnd::new("m:t"))).unwrap();
        writer.write_event(Event::End(quick_xml::events::BytesEnd::new("m:r"))).unwrap();
    }
}
