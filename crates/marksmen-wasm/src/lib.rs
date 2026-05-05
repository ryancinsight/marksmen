use wasm_bindgen::prelude::*;
use marksmen_core::config::Config;
use marksmen_core::parsing::parser;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

/// Converts Markdown text into structural HTML.
#[wasm_bindgen]
pub fn md_to_html(markdown: &str) -> Result<String, JsValue> {
    use marksmen_core::config::frontmatter::parse_frontmatter;
    
    let (body, fm) = parse_frontmatter(markdown).unwrap_or((markdown, Default::default()));
    let config = Config::default().merge_frontmatter(&fm);
    
    let events = parser::parse(body);
    match marksmen_html::convert(events, &config) {
        Ok(html) => Ok(html),
        Err(e) => Err(JsValue::from_str(&e.to_string())),
    }
}

/// Converts structural HTML back into Markdown.
#[wasm_bindgen]
pub fn html_to_md(html: &str) -> Result<String, JsValue> {
    match marksmen_html_read::parse_html(html) {
        Ok(md) => Ok(md),
        Err(e) => Err(JsValue::from_str(&e.to_string())),
    }
}


