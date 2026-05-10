use marksmen_core::config::Config;
use marksmen_mermaid::graph::directed_graph;
use marksmen_mermaid::layout::{coordinate_assign, crossing_reduction, rank_assignment};
use marksmen_mermaid::parsing::parser;
use marksmen_xml::escape;
use pulldown_cmark::{Event, Tag, TagEnd};
use std::path::Path;

pub(crate) fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{}=\"", attr);
    if let Some(start) = tag.find(&needle) {
        let remaining = &tag[start + needle.len()..];
        if let Some(end) = remaining.find('"') {
            return Some(remaining[..end].to_string());
        }
    }
    None
}

/// Iterates structurally over the parsed `Event` stream and sequentially constructs
/// the native `<text:p>`, `<text:h>`, and `<text:span>` elements mapping directly
/// into the OpenDocument core `<office:text>` container.
pub fn translate_events<'a>(
    events: &[Event<'a>],
    config: &Config,
    input_dir: &Path,
) -> (String, Vec<String>, Vec<(String, Vec<u8>)>, String) {
    let mut output = String::with_capacity(events.len() * 32);
    let mut math_objects = Vec::new();
    let mut images = Vec::new();
    let mut tracked_changes = String::with_capacity(512);
    let mut change_counter = 0;

    if !events.is_empty() {
        tracked_changes.push_str("<text:tracked-changes>\n");
    }

    let mut in_blockquote = false;

    // Inject YAML Frontmatter (Title Page)
    if !config.title.is_empty() {
        output.push_str(&format!(
            "<text:p text:style-name=\"T_Title\">{}</text:p>\n",
            escape(&config.title)
        ));
    }
    if !config.author.is_empty() {
        // author is a std::string::String in Config, so we just use it directly
        output.push_str(&format!(
            "<text:p text:style-name=\"T_Author\">{}</text:p>\n",
            escape(&config.author)
        ));
    }
    if !config.date.is_empty() {
        output.push_str(&format!(
            "<text:p text:style-name=\"T_Author\">{}</text:p>\n",
            escape(&config.date)
        ));
    }
    if !config.title.is_empty() {
        // Isolate the Title page structurally via explicit fo:break-before page mapping
        output.push_str("<text:p text:style-name=\"P_Break\"/>\n");
    }

    let mut in_mermaid_block = false;
    let mut current_mermaid_source = String::new();
    let mut table_alignments: Vec<pulldown_cmark::Alignment> = Vec::new();
    let mut current_cell_idx = 0;
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
            Event::Start(Tag::Table(ref aligns)) => {
                table_alignments = aligns.clone();
                output.push_str("<table:table table:style-name=\"Table_Full\">\n")
            }
            Event::End(TagEnd::Table) => {
                table_alignments.clear();
                output.push_str("</table:table>\n");
            }
            Event::Start(Tag::TableHead) => {
                current_cell_idx = 0;
                output.push_str("<table:table-header-rows>\n");
            }
            Event::End(TagEnd::TableHead) => output.push_str("</table:table-header-rows>\n"),
            Event::Start(Tag::TableRow) => {
                current_cell_idx = 0;
                output.push_str("<table:table-row>\n");
            }
            Event::End(TagEnd::TableRow) => output.push_str("</table:table-row>\n"),
            Event::Start(Tag::TableCell) => {
                let align = table_alignments
                    .get(current_cell_idx)
                    .unwrap_or(&pulldown_cmark::Alignment::None);
                let p_style = match align {
                    pulldown_cmark::Alignment::Center => " text:style-name=\"P_Center\"",
                    pulldown_cmark::Alignment::Right => " text:style-name=\"P_Right\"",
                    pulldown_cmark::Alignment::Left => " text:style-name=\"P_Left\"",
                    pulldown_cmark::Alignment::None => "",
                };
                output.push_str(&format!("<table:table-cell><text:p{}>", p_style));
            }
            Event::End(TagEnd::TableCell) => {
                output.push_str("</text:p></table:table-cell>\n");
                current_cell_idx += 1;
            }

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
            Event::Start(Tag::Emphasis) => {
                output.push_str("<text:span text:style-name=\"S_Italic\">")
            }
            Event::End(TagEnd::Emphasis) => output.push_str("</text:span>"),
            Event::Start(Tag::Strikethrough) => {
                output.push_str("<text:span text:style-name=\"S_Strikethrough\">")
            }
            Event::End(TagEnd::Strikethrough) => output.push_str("</text:span>"),
            Event::Start(Tag::Superscript) => {
                output.push_str("<text:span text:style-name=\"S_Superscript\">")
            }
            Event::End(TagEnd::Superscript) => output.push_str("</text:span>"),
            Event::Start(Tag::Subscript) => {
                output.push_str("<text:span text:style-name=\"S_Subscript\">")
            }
            Event::End(TagEnd::Subscript) => output.push_str("</text:span>"),

            Event::Start(Tag::BlockQuote(_)) => in_blockquote = true,
            Event::End(TagEnd::BlockQuote(_)) => in_blockquote = false,

            // Code Blocks
            Event::Start(Tag::CodeBlock(ref kind)) => {
                if let pulldown_cmark::CodeBlockKind::Fenced(lang) = kind {
                    if lang.as_ref() == "mermaid" {
                        in_mermaid_block = true;
                        current_mermaid_source.clear();
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
                    in_mermaid_block = false;

                    let ast = match parser::parse(&current_mermaid_source) {
                        Ok(a) => a,
                        Err(_) => {
                            output.push_str(&format!(
                                "<text:p text:style-name=\"P_CodeBlock\">{}</text:p>\n",
                                escape(&current_mermaid_source)
                            ));
                            continue;
                        }
                    };

                    let directed_graph = directed_graph::ast_to_graph(ast);
                    let mut ranked_graph = rank_assignment::assign_ranks(&directed_graph);
                    crossing_reduction::minimize_crossings(&mut ranked_graph);
                    let spaced_graph = coordinate_assign::assign_coordinates(&ranked_graph);

                    let svg_result = marksmen_render::mermaid::render_graph_to_svg(&spaced_graph);
                    if let Some((png_bytes, _width, _height)) =
                        marksmen_render::svg_bytes_to_png(svg_result.as_bytes())
                    {
                        let filename = format!("mermaid_{}.png", images.len() + 1);
                        let image_id = format!("Pictures/{}", filename);
                        images.push((image_id.clone(), png_bytes));

                        output.push_str(&format!(
                            r#"<text:p text:style-name="P_DisplayMath"><draw:frame draw:z-index="1" text:anchor-type="paragraph" svg:width="6in"><draw:image xlink:href="{}" xlink:type="simple" xlink:show="embed" xlink:actuate="onLoad"/></draw:frame></text:p>
"#,
                            escape(&image_id)
                        ));
                    } else {
                        output.push_str("<text:p>[Mermaid Graph Error]</text:p>\n");
                    }
                } else {
                    output.push_str("<text:line-break/>```</text:p>\n");
                }
            }
            Event::Code(c) => {
                if in_mermaid_block {
                    current_mermaid_source.push_str(c.as_ref());
                } else {
                    output.push_str(&format!(
                        "<text:span text:style-name=\"S_Code\">{}</text:span>",
                        escape(c.as_ref())
                    ));
                }
            }

            Event::Text(t) => {
                if in_mermaid_block {
                    current_mermaid_source.push_str(t.as_ref());
                } else {
                    output.push_str(&escape(t.as_ref()));
                }
            }
            Event::SoftBreak | Event::HardBreak => output.push_str("<text:line-break/>"),

            Event::Rule => output.push_str("<text:p text:style-name=\"P_Break\"/>\n"),

            // --- HTML Semantic Mappings ---
            Event::Html(html) => {
                let original_tag = html.as_ref();
                let tag = original_tag.to_lowercase();
                if tag.starts_with("<u") {
                    output.push_str("<text:span text:style-name=\"S_Underline\">");
                } else if tag.starts_with("</u") {
                    output.push_str("</text:span>");
                } else if tag.starts_with("<sub") {
                    output.push_str("<text:span text:style-name=\"S_Sub\">");
                } else if tag.starts_with("</sub") {
                    output.push_str("</text:span>");
                } else if tag.starts_with("<sup") {
                    output.push_str("<text:span text:style-name=\"S_Sup\">");
                } else if tag.starts_with("</sup") {
                    output.push_str("</text:span>");
                } else if tag.starts_with("<table") {
                    output.push_str("<table:table table:style-name=\"Table_Full\">\n");
                } else if tag.starts_with("</table") {
                    output.push_str("</table:table>\n");
                } else if tag.starts_with("<thead") {
                    output.push_str("<table:table-header-rows>\n");
                } else if tag.starts_with("</thead") {
                    output.push_str("</table:table-header-rows>\n");
                } else if tag.starts_with("<tr") {
                    output.push_str("<table:table-row>\n");
                } else if tag.starts_with("</tr") {
                    output.push_str("</table:table-row>\n");
                } else if tag.starts_with("<td") || tag.starts_with("<th") {
                    if let Some(colspan) = extract_attr(original_tag, "colspan") {
                        let colspan_attr = format!(" table:number-columns-spanned=\"{}\"", colspan);
                        output.push_str(&format!("<table:table-cell{}><text:p>", colspan_attr));
                    } else {
                        output.push_str("<table:table-cell><text:p>");
                    }
                } else if tag.starts_with("</td") || tag.starts_with("</th") {
                    output.push_str("</text:p></table:table-cell>\n");
                } else if tag.starts_with("<ins") || tag.starts_with("<del") {
                    let author = extract_attr(original_tag, "data-author")
                        .unwrap_or_else(|| "Unknown".to_string());
                    let date = extract_attr(original_tag, "data-date")
                        .unwrap_or_else(|| "2026-04-28T00:00:00Z".to_string());
                    change_counter += 1;
                    let change_id = format!("ct{}", change_counter);
                    let change_type = if tag.starts_with("<ins") {
                        "insertion"
                    } else {
                        "deletion"
                    };

                    tracked_changes.push_str(&format!(
                        r#"  <text:changed-region text:id="{}">
    <text:{}>
      <office:change-info>
        <dc:creator>{}</dc:creator>
        <dc:date>{}</dc:date>
      </office:change-info>
    </text:{}>
  </text:changed-region>
"#,
                        change_id,
                        change_type,
                        escape(&author),
                        escape(&date),
                        change_type
                    ));

                    output.push_str(&format!(
                        "<text:change-start text:change-id=\"{}\"/>",
                        change_id
                    ));
                } else if tag.starts_with("</ins") || tag.starts_with("</del") {
                    // Match the last opened change
                    output.push_str(&format!(
                        "<text:change-end text:change-id=\"ct{}\"/>",
                        change_counter
                    ));
                }
            }

            // --- Math Equations ---
            Event::InlineMath(latex) => {
                match latex2mathml::latex_to_mathml(
                    latex.as_ref(),
                    latex2mathml::DisplayStyle::Inline,
                ) {
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
                        output.push_str(&format!(
                            "<text:span text:style-name=\"S_MathInline\">{}</text:span>",
                            escape(latex.as_ref())
                        ));
                    }
                }
            }
            Event::DisplayMath(latex) => {
                match latex2mathml::latex_to_mathml(
                    latex.as_ref(),
                    latex2mathml::DisplayStyle::Block,
                ) {
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
                        output.push_str(&format!(
                            "<text:p text:style-name=\"P_DisplayMath\">{}</text:p>\n",
                            escape(latex.as_ref())
                        ));
                    }
                }
            }

            // --- Images ---
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                let img_path_str = dest_url.as_ref();
                let mut image_bytes = None;
                let mut filename = format!("image_{}.png", images.len() + 1);

                if img_path_str.starts_with("data:image/") {
                    if let Some(comma_idx) = img_path_str.find(',') {
                        let base64_data = &img_path_str[comma_idx + 1..];
                        use base64::Engine as _;
                        if let Ok(raw_bytes) =
                            base64::engine::general_purpose::STANDARD.decode(base64_data)
                        {
                            image_bytes = Some(raw_bytes);
                        }
                    }
                } else {
                    let img_path = input_dir.join(img_path_str);
                    if let Ok(bytes) = std::fs::read(&img_path) {
                        image_bytes = Some(bytes);
                        if let Some(name) = img_path.file_name() {
                            filename = name.to_string_lossy().into_owned();
                        }
                    }
                }

                // If we can load the image, embed it. Otherwise fallback to text placeholder.
                if let Some(bytes) = image_bytes {
                    let image_id = format!("Pictures/{}", filename);
                    images.push((image_id.clone(), bytes));

                    output.push_str(&format!(
                        r#"<text:p text:style-name="P_DisplayMath"><draw:frame draw:z-index="1" text:anchor-type="paragraph" svg:width="6in"><draw:image xlink:href="{}" xlink:type="simple" xlink:show="embed" xlink:actuate="onLoad"/></draw:frame></text:p>
"#,
                        escape(&image_id)
                    ));
                    if !title.is_empty() {
                        output.push_str(&format!(
                            "<text:p text:style-name=\"S_Italic\">{}</text:p>\n",
                            escape(title.as_ref())
                        ));
                    }
                } else {
                    // Fallback
                    output.push_str(&format!(
                        "<text:p>[Figure: {}]</text:p>\n",
                        escape(dest_url.as_ref())
                    ));
                    if !title.is_empty() {
                        output.push_str(&format!(
                            "<text:p text:style-name=\"S_Italic\">{}</text:p>\n",
                            escape(title.as_ref())
                        ));
                    }
                }

                let alt_text = if title.is_empty() {
                    "Image"
                } else {
                    title.as_ref()
                };
                output.push_str(&format!(
                    "<text:p text:style-name=\"P_HiddenMeta\">![{}]({})</text:p>\n",
                    escape(alt_text),
                    escape(dest_url.as_ref())
                ));
            }
            Event::End(TagEnd::Image) => {}

            // --- Links ---
            Event::Start(Tag::Link { dest_url, .. }) => {
                output.push_str(&format!(
                    "<text:a xlink:type=\"simple\" xlink:href=\"{}\">",
                    escape(dest_url.as_ref())
                ));
            }
            Event::End(TagEnd::Link) => output.push_str("</text:a>"),

            _ => {
                // Pass on other nodes mathematically unimplemented for the ODT phase
            }
        }
    }

    if !events.is_empty() {
        tracked_changes.push_str("</text:tracked-changes>\n");
    }

    (output, math_objects, images, tracked_changes)
}
