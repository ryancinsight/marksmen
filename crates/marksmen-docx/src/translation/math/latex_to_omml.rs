use docx_rs::*;

/// Trait defining analytical translation from LaTeX tokens to structural OMML xml streams.
pub trait OmmlRenderer {
    /// Render internal structured XML into an `oMathPara` block for Word.
    fn render_display_math(&self, latex: &str) -> (String, String);

    /// Render internal structured XML into an inline `oMath` block.
    fn render_inline_math(&self, latex: &str) -> (String, String);
}

pub struct LatexToOmmlTranslator;

impl LatexToOmmlTranslator {
    pub fn new() -> Self {
        Self {}
    }

    /// Evaluates LaTeX into raw OpenXML math strings.
    pub fn build_xml(&self, latex: &str) -> String {
        crate::translation::math::ast::OmmlAstGenerator::translate(latex)
    }
}

impl OmmlRenderer for LatexToOmmlTranslator {
    fn render_display_math(&self, latex: &str) -> (String, String) {
        // Wrap the OMML payload in a w:p and m:oMathPara to establish the Display bounds.
        let xml = self.build_xml(latex);
        
        let mut math_block = String::new();
        math_block.push_str(r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">"#);
        math_block.push_str("<m:oMathPara>");
        math_block.push_str(&xml);
        math_block.push_str("</m:oMathPara>");
        math_block.push_str("</w:p>");
        
        let id = format!("math_{}", uuid::Uuid::new_v4());
        (id, math_block)
    }

    fn render_inline_math(&self, latex: &str) -> (String, String) {
        // Wrap the OMML payload in m:oMath for inline execution within a parent w:p span
        let xml = self.build_xml(latex);
        let id = format!("math_{}", uuid::Uuid::new_v4());
        
        let mut inline_block = String::new();
        inline_block.push_str(r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">"#);
        inline_block.push_str(&xml);
        inline_block.push_str("</w:p>");
        (id, inline_block)
    }
}
