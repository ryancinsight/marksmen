use crate::schema::{Style, RenderingElement, Layout, Text};
use crate::model::Reference;
use std::collections::HashMap;

/// The evaluation context holds the style, the current reference, and formatting state.
pub struct Context<'a> {
    pub style: &'a Style,
    pub reference: &'a Reference,
    pub macros: HashMap<String, &'a Vec<RenderingElement>>,
}

impl<'a> Context<'a> {
    pub fn new(style: &'a Style, reference: &'a Reference) -> Self {
        let mut macros = HashMap::new();
        for m in &style.macros {
            macros.insert(m.name.clone(), &m.elements);
        }
        Self {
            style,
            reference,
            macros,
        }
    }
}

/// Evaluates a layout into a formatted string.
pub fn evaluate_layout(layout: &Layout, ctx: &Context) -> String {
    let mut output = String::new();
    if let Some(prefix) = &layout.prefix {
        output.push_str(prefix);
    }
    
    let mut elements_out = Vec::new();
    for el in &layout.elements {
        if let Some(rendered) = evaluate_element(el, ctx).filter(|r| !r.is_empty()) {
            elements_out.push(rendered);
        }
    }
    
    let delimiter = layout.delimiter.as_deref().unwrap_or("");
    output.push_str(&elements_out.join(delimiter));
    
    if let Some(suffix) = &layout.suffix {
        output.push_str(suffix);
    }
    
    output
}

/// Recursively evaluates a rendering element.
pub fn evaluate_element(element: &RenderingElement, ctx: &Context) -> Option<String> {
    match element {
        RenderingElement::Text(text_node) => evaluate_text(text_node, ctx),
        RenderingElement::Date(date) => {
            let date_val = match date.variable.as_str() {
                "issued" => ctx.reference.issued.as_ref(),
                "accessed" => ctx.reference.accessed.as_ref(),
                _ => None,
            };
            if let Some(parts) = date_val.and_then(|v| v.date_parts.first()) {
                let year = parts.first().map(|y| y.to_string()).unwrap_or_default();
                let month = parts.get(1).map(|m| format!("{:02}", m)).unwrap_or_default();
                let day = parts.get(2).map(|d| format!("{:02}", d)).unwrap_or_default();
                
                let mut out = year;
                if !month.is_empty() {
                    out.push('-');
                    out.push_str(&month);
                }
                if !day.is_empty() {
                    out.push('-');
                    out.push_str(&day);
                }
                return Some(out);
            }
            None
        }
        RenderingElement::Number(_) => {
            // TODO: Number rendering
            Some("[Number]".to_string())
        }
        RenderingElement::Names(names) => {
            let authors_opt = match names.variable.as_str() {
                "author" => ctx.reference.author.as_ref(),
                "editor" => ctx.reference.editor.as_ref(),
                "translator" => ctx.reference.translator.as_ref(),
                _ => None,
            };
            
            if let Some(authors) = authors_opt {
                if authors.is_empty() {
                    return None;
                }
                
                let mut formatted_names = Vec::new();
                for author in authors {
                    let family = author.family.as_deref().unwrap_or("");
                    let given = author.given.as_deref().unwrap_or("");
                    let literal = author.literal.as_deref().unwrap_or("");
                    
                    if !literal.is_empty() {
                        formatted_names.push(literal.to_string());
                    } else if !family.is_empty() && !given.is_empty() {
                        // Default format: "Family, G."
                        let initial = given.chars().next().unwrap_or('?').to_string();
                        formatted_names.push(format!("{}, {}.", family, initial));
                    } else {
                        formatted_names.push(format!("{}{}", family, given));
                    }
                }
                
                let delimiter = if let Some(n) = &names.name {
                    n.delimiter.clone().unwrap_or_else(|| ", ".to_string())
                } else {
                    ", ".to_string()
                };
                
                let out = if formatted_names.len() > 1 {
                    let last = formatted_names.pop().unwrap();
                    let and_symbol = if let Some(n) = &names.name {
                        if let Some(and) = &n.and {
                            if and == "symbol" { " & " } else { " and " }
                        } else {
                            " "
                        }
                    } else {
                        " "
                    };
                    format!("{}{}{}", formatted_names.join(&delimiter), and_symbol, last)
                } else {
                    formatted_names[0].clone()
                };
                
                Some(out)
            } else {
                None
            }
        }
        RenderingElement::Label(_) => {
            // TODO: Label lookup via locale terms
            Some("[Label]".to_string())
        }
        RenderingElement::Group(group) => {
            let mut out = Vec::new();
            for el in &group.elements {
                if let Some(rendered) = evaluate_element(el, ctx).filter(|r| !r.is_empty()) {
                    out.push(rendered);
                }
            }
            if out.is_empty() {
                None
            } else {
                let delim = group.delimiter.as_deref().unwrap_or("");
                Some(out.join(delim))
            }
        }
        RenderingElement::Choose(choose) => {
            for condition in &choose.if_block {
                if evaluate_condition(condition, ctx) {
                    let mut out = String::new();
                    for el in &condition.elements {
                        if let Some(rendered) = evaluate_element(el, ctx) {
                            out.push_str(&rendered);
                        }
                    }
                    return Some(out);
                }
            }
            
            for condition in &choose.else_if_block {
                if evaluate_condition(condition, ctx) {
                    let mut out = String::new();
                    for el in &condition.elements {
                        if let Some(rendered) = evaluate_element(el, ctx) {
                            out.push_str(&rendered);
                        }
                    }
                    return Some(out);
                }
            }
            
            if let Some(else_block) = &choose.else_block {
                let mut out = String::new();
                for el in &else_block.elements {
                    if let Some(rendered) = evaluate_element(el, ctx) {
                        out.push_str(&rendered);
                    }
                }
                return Some(out);
            }
            
            None
        }
    }
}

fn evaluate_text(text: &Text, ctx: &Context) -> Option<String> {
    let mut content = None;
    
    if let Some(val) = &text.value {
        content = Some(val.clone());
    } else if let Some(var) = &text.variable {
        content = match var.as_str() {
            "title" => ctx.reference.title.clone(),
            "container-title" => ctx.reference.container_title.clone(),
            "publisher" => ctx.reference.publisher.clone(),
            "publisher-place" => ctx.reference.publisher_place.clone(),
            "page" => ctx.reference.page.clone(),
            "volume" => ctx.reference.volume.clone(),
            "issue" => ctx.reference.issue.clone(),
            "doi" => ctx.reference.doi.clone(),
            "url" => ctx.reference.url.clone(),
            "isbn" => ctx.reference.isbn.clone(),
            "issn" => ctx.reference.issn.clone(),
            _ => None,
        };
    } else if let Some(mac_name) = &text.macro_name {
        if let Some(elements) = ctx.macros.get(mac_name) {
            let mut out = String::new();
            for el in *elements {
                if let Some(rendered) = evaluate_element(el, ctx) {
                    out.push_str(&rendered);
                }
            }
            content = Some(out);
        }
    } else if let Some(term) = &text.term {
        // TODO: Locale term lookup
        content = Some(format!("[term:{}]", term));
    }
    
    if let Some(ref c) = content {
        if c.is_empty() {
            return None;
        }
        let mut final_out = String::new();
        if let Some(prefix) = &text.prefix {
            final_out.push_str(prefix);
        }
        
        // TODO: apply font-style, text-case, font-weight formatting via markup wrappers
        if text.quotes.unwrap_or(false) {
            final_out.push_str(&format!("\"{}\"", c));
        } else {
            final_out.push_str(c);
        }
        
        if let Some(suffix) = &text.suffix {
            final_out.push_str(suffix);
        }
        Some(final_out)
    } else {
        None
    }
}

fn evaluate_condition(condition: &crate::schema::IfBlock, ctx: &Context) -> bool {
    // Determine if the variable is present
    if let Some(var) = &condition.variable_match {
        let is_present = match var.as_str() {
            "title" => ctx.reference.title.is_some(),
            "container-title" => ctx.reference.container_title.is_some(),
            "publisher" => ctx.reference.publisher.is_some(),
            "issued" => ctx.reference.issued.is_some(),
            "author" => ctx.reference.author.is_some(),
            "editor" => ctx.reference.editor.is_some(),
            _ => false,
        };
        
        // Handle "match" attribute ("all", "any", "none")
        // For a single variable, "any" and "all" are equivalent to `is_present`.
        let match_type = condition.match_condition.as_deref().unwrap_or("all");
        match match_type {
            "none" => return !is_present,
            _ => return is_present,
        }
    }
    
    if let Some(typ) = &condition.type_match {
        let is_match = ctx.reference.r#type == *typ;
        let match_type = condition.match_condition.as_deref().unwrap_or("all");
        match match_type {
            "none" => return !is_match,
            _ => return is_match,
        }
    }
    
    false
}
