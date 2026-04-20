use pulldown_cmark::{Event, Tag, TagEnd};
use marksmen_core::config::Config;
use marksmen_xml::escape;
use std::path::Path;

/// Iterates structurally over the parsed `Event` stream and sequentially constructs
/// the native `<text:p>`, `<text:h>`, and `<text:span>` elements mapping directly
/// into the OpenDocument core `<office:text>` container.
pub fn translate_events<'a>(events: &[Event<'a>], config: &Config, _input_dir: &Path) -> (String, Vec<String>) {
    let mut output = String::with_capacity(events.len() * 32);
    let mut math_objects = Vec::new();
    let mut in_blockquote = false;
    
    // Inject YAML Frontmatter (Title Page)
    if !config.title.is_empty() {
        output.push_str(&format!("<text:p text:style-name=\"T_Title\">{}</text:p>\n", escape(&config.title)));
    }
    if !config.author.is_empty() {
        // author is a std::string::String in Config, so we just use it directly
        output.push_str(&format!("<text:p text:style-name=\"T_Author\">{}</text:p>\n", escape(&config.author)));
    }
    if !config.date.is_empty() {
        output.push_str(&format!("<text:p text:style-name=\"T_Author\">{}</text:p>\n", escape(&config.date)));
    }
    if !config.title.is_empty() {
        // Isolate the Title page structurally via explicit fo:break-before page mapping
        output.push_str("<text:p text:style-name=\"P_Break\"/>\n");
    }
    
    let mut in_mermaid_block = false;
    // List state: track ordered/unordered per nesting level.
    // ODF nesting: <text:list-item> can contain <text:p> then a nested <text:list>.
    // We close </text:p> before emitting a nested <text:list>, reopen if needed.
    let mut list_ordered_stack: Vec<bool> = Vec::new();
    let mut list_item_has_open_p = false;
    for event in events {
        match event {
            Event::Start(Tag::Paragraph) => {
                if in_blockquote {
                    output.push_str("<text:p text:style-name=\"P_Quote\">");
                } else {
                    output.push_str("<text:p>");
                }
            }
            Event::End(TagEnd::Paragraph) => output.push_str("</text:p>\n"),
            Event::Start(Tag::Heading { level, .. }) => {
                let h_level = *level as u8;
                output.push_str(&format!("<text:h text:outline-level=\"{}\">", h_level));
            }
            Event::End(TagEnd::Heading(_)) => output.push_str("</text:h>\n"),
            
            // --- Tables ---
            Event::Start(Tag::Table(_)) => output.push_str("<table:table table:style-name=\"Table_Full\">\n"),
            Event::End(TagEnd::Table) => output.push_str("</table:table>\n"),
            Event::Start(Tag::TableHead) => output.push_str("<table:table-header-rows>\n"),
            Event::End(TagEnd::TableHead) => output.push_str("</table:table-header-rows>\n"),
            Event::Start(Tag::TableRow) => output.push_str("<table:table-row>\n"),
            Event::End(TagEnd::TableRow) => output.push_str("</table:table-row>\n"),
            Event::Start(Tag::TableCell) => output.push_str("<table:table-cell><text:p>"),
            Event::End(TagEnd::TableCell) => output.push_str("</text:p></table:table-cell>\n"),

            // --- Lists ---
            Event::Start(Tag::List(start_num)) => {
                let is_ordered = start_num.is_some();
                list_ordered_stack.push(is_ordered);
                let style = if is_ordered { "L_Numbered" } else { "L_Bullet" };
                // If we are inside an open <text:p> of a parent item, close it first.
                if list_item_has_open_p {
                    output.push_str("</text:p>\n");
                    list_item_has_open_p = false;
                }
                output.push_str(&format!("<text:list text:style-name=\"{}\">\n", style));
            }
            Event::End(TagEnd::List(_)) => {
                list_ordered_stack.pop();
                output.push_str("</text:list>\n");
            }
            Event::Start(Tag::Item) => {
                output.push_str("<text:list-item>");
                // Open the item's paragraph immediately; it will be closed when a
                // sub-list starts OR when End(Item) fires with no sub-list.
                output.push_str("<text:p>");
                list_item_has_open_p = true;
            }
            Event::End(TagEnd::Item) => {
                if list_item_has_open_p {
                    output.push_str("</text:p>");
                    list_item_has_open_p = false;
                }
                output.push_str("</text:list-item>\n");
            }
            
            // --- Text Formatting ---
            Event::Start(Tag::Strong) => output.push_str("<text:span text:style-name=\"S_Bold\">"),
            Event::End(TagEnd::Strong) => output.push_str("</text:span>"),
            Event::Start(Tag::Emphasis) => output.push_str("<text:span text:style-name=\"S_Italic\">"),
            Event::End(TagEnd::Emphasis) => output.push_str("</text:span>"),

            Event::Start(Tag::BlockQuote(_)) => in_blockquote = true,
            Event::End(TagEnd::BlockQuote(_)) => in_blockquote = false,
            
            // Code Blocks
            Event::Start(Tag::CodeBlock(ref kind)) => {
                if let pulldown_cmark::CodeBlockKind::Fenced(lang) = kind {
                    if lang.as_ref() == "mermaid" {
                        in_mermaid_block = true;
                        output.push_str("<text:hidden-paragraph text:is-hidden=\"true\" text:condition=\"ooow:TRUE\">```mermaid<text:line-break/>");
                        continue;
                    }
                }
                output.push_str("<text:p text:style-name=\"P_CodeBlock\">");
                if let pulldown_cmark::CodeBlockKind::Fenced(lang) = kind {
                    if !lang.is_empty() {
                        output.push_str(&format!("```{}<text:line-break/>", escape(lang.as_ref())));
                    } else {
                        output.push_str("```<text:line-break/>");
                    }
                } else {
                    output.push_str("```<text:line-break/>");
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if in_mermaid_block {
                    output.push_str("<text:line-break/>```</text:hidden-paragraph>\n");
                    in_mermaid_block = false;
                } else {
                    output.push_str("<text:line-break/>```</text:p>\n");
                }
            }
            Event::Code(c) => output.push_str(&format!("<text:span text:style-name=\"S_Code\">{}</text:span>", escape(c.as_ref()))),
            
            Event::Text(t) => output.push_str(&escape(t.as_ref())),
            Event::SoftBreak | Event::HardBreak => output.push_str("<text:line-break/>"),
            
            Event::Rule => output.push_str("<text:p text:style-name=\"P_Rule\">---</text:p>\n"),

            // --- HTML Semantic Mappings ---
            Event::Html(html) => {
                let tag = html.as_ref().to_lowercase();
                if tag.starts_with("<u") { output.push_str("<text:span text:style-name=\"S_Underline\">"); }
                else if tag.starts_with("</u") { output.push_str("</text:span>"); }
                else if tag.starts_with("<sub") { output.push_str("<text:span text:style-name=\"S_Sub\">"); }
                else if tag.starts_with("</sub") { output.push_str("</text:span>"); }
                else if tag.starts_with("<sup") { output.push_str("<text:span text:style-name=\"S_Sup\">"); }
                else if tag.starts_with("</sup") { output.push_str("</text:span>"); }
            }

            // --- Math Equations ---
            Event::InlineMath(latex) => {
                match latex2mathml::latex_to_mathml(latex.as_ref(), latex2mathml::DisplayStyle::Inline) {
                    Ok(mathml) => {
                        let object_id = format!("Object {}", math_objects.len() + 1);
                        math_objects.push(mathml);
                        output.push_str(&format!(
                            "<draw:frame draw:z-index=\"0\" text:anchor-type=\"as-char\"><draw:object xlink:href=\"./{}\" xlink:type=\"simple\"/></draw:frame>",
                            object_id
                        ));
                        output.push_str(&format!(
                            "<text:span text:style-name=\"S_HiddenMeta\">{}</text:span>",
                            escape(latex.as_ref())
                        ));
                    }
                    Err(_) => {
                        output.push_str(&format!("<text:span text:style-name=\"S_MathInline\">{}</text:span>", escape(latex.as_ref())));
                    }
                }
            }
            Event::DisplayMath(latex) => {
                match latex2mathml::latex_to_mathml(latex.as_ref(), latex2mathml::DisplayStyle::Block) {
                    Ok(mathml) => {
                        let object_id = format!("Object {}", math_objects.len() + 1);
                        math_objects.push(mathml);
                        output.push_str(&format!(
                            "<text:p text:style-name=\"P_DisplayMath\"><draw:frame draw:z-index=\"0\" text:anchor-type=\"paragraph\"><draw:object xlink:href=\"./{}\" xlink:type=\"simple\"/></draw:frame></text:p>\n",
                            object_id
                        ));
                        output.push_str(&format!(
                            "<text:p text:style-name=\"P_HiddenMeta\">{}</text:p>\n",
                            escape(latex.as_ref())
                        ));
                    }
                    Err(_) => {
                        output.push_str(&format!("<text:p text:style-name=\"P_DisplayMath\">{}</text:p>\n", escape(latex.as_ref())));
                    }
                }
            }

            // --- Images ---
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                // ODT images require embedding via draw:frame + draw:image with xlink:href
                // For now, emit a text placeholder with the path
                output.push_str(&format!("<text:p>[Figure: {}]</text:p>\n", escape(dest_url.as_ref())));
                if !title.is_empty() {
                    output.push_str(&format!("<text:p text:style-name=\"S_Italic\">{}</text:p>\n", escape(title.as_ref())));
                }
                let alt_text = if title.is_empty() { "Image" } else { title.as_ref() };
                output.push_str(&format!(
                    "<text:p text:style-name=\"P_HiddenMeta\">![{}]({})</text:p>\n",
                    escape(alt_text),
                    escape(dest_url.as_ref())
                ));
            }
            Event::End(TagEnd::Image) => {}

            // --- Links ---
            Event::Start(Tag::Link { dest_url, .. }) => {
                output.push_str(&format!("<text:a xlink:type=\"simple\" xlink:href=\"{}\">", escape(dest_url.as_ref())));
            }
            Event::End(TagEnd::Link) => output.push_str("</text:a>"),

            _ => {
                // Pass on other nodes mathematically unimplemented for the ODT phase
            }
        }
    }
    
    (output, math_objects)
}
