//! Core diffing engine for `marksmen` workspace.
//!
//! Provides mathematically rigorous symmetric differences over AST strings, generating HTML payloads.

use marksmen_core::config::Config;
use similar::{DiffOp, TextDiff};
use pulldown_cmark::{Parser, Event};

/// Derives the symmetric structural difference between two Markdown artifacts.
/// Returns the diff safely rendered as HTML to prevent Markdown syntax breakage.
pub fn diff_markdown(old: &str, new: &str) -> String {
    let old_blocks = get_top_level_blocks(old);
    let new_blocks = get_top_level_blocks(new);

    let diff = TextDiff::from_slices(&old_blocks, &new_blocks);
    let config = Config::default();
    let mut out = String::new();

    for op in diff.ops() {
        match op {
            DiffOp::Equal { old_index, len, .. } => {
                for i in 0..*len {
                    out.push_str(&render_block(old_blocks[*old_index + i], &config));
                }
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                for i in 0..*old_len {
                    out.push_str(&format!(
                        "<div class=\"diff-del\">\n{}\n</div>",
                        render_block(old_blocks[*old_index + i], &config)
                    ));
                }
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for i in 0..*new_len {
                    out.push_str(&format!(
                        "<div class=\"diff-ins\">\n{}\n</div>",
                        render_block(new_blocks[*new_index + i], &config)
                    ));
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                if *old_len == *new_len {
                    for i in 0..*old_len {
                        let old_block = old_blocks[*old_index + i];
                        let new_block = new_blocks[*new_index + i];
                        let inline_diff = TextDiff::from_words(old_block, new_block);
                        let mut diffed_md = String::new();
                        
                        for change in inline_diff.iter_all_changes() {
                            match change.tag() {
                                similar::ChangeTag::Delete => {
                                    diffed_md.push_str("<del>");
                                    diffed_md.push_str(&change.to_string_lossy());
                                    diffed_md.push_str("</del>");
                                }
                                similar::ChangeTag::Insert => {
                                    diffed_md.push_str("<ins>");
                                    diffed_md.push_str(&change.to_string_lossy());
                                    diffed_md.push_str("</ins>");
                                }
                                similar::ChangeTag::Equal => {
                                    diffed_md.push_str(&change.to_string_lossy());
                                }
                            }
                        }
                        
                        out.push_str(&render_block(&diffed_md, &config));
                    }
                } else {
                    for i in 0..*old_len {
                        out.push_str(&format!(
                            "<div class=\"diff-del\">\n{}\n</div>",
                            render_block(old_blocks[*old_index + i], &config)
                        ));
                    }
                    for i in 0..*new_len {
                        out.push_str(&format!(
                            "<div class=\"diff-ins\">\n{}\n</div>",
                            render_block(new_blocks[*new_index + i], &config)
                        ));
                    }
                }
            }
        }
    }

    out
}

fn get_top_level_blocks(src: &str) -> Vec<&str> {
    let parser = Parser::new(src).into_offset_iter();
    let mut blocks = Vec::new();
    let mut depth = 0;
    let mut current_start = None;

    for (event, range) in parser {
        match event {
            Event::Start(_) => {
                if depth == 0 {
                    current_start = Some(range.start);
                }
                depth += 1;
            }
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = current_start {
                        blocks.push(&src[start..range.end]);
                        current_start = None;
                    }
                }
            }
            _ => {
                if depth == 0 {
                    blocks.push(&src[range.start..range.end]);
                }
            }
        }
    }
    blocks
}

fn render_block(text: &str, _config: &Config) -> String {
    let mut html = String::new();
    let parser = pulldown_cmark::Parser::new(text);
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_structural_blocks() {
        let old = "# Header\n\nParagraph 1.\n\nAnother block.";
        let new = "# Header\n\nParagraph 2.\n\nAnother new block.";
        let old_blocks = get_top_level_blocks(old);
        let new_blocks = get_top_level_blocks(new);
        println!("OLD BLOCKS: {:?}", old_blocks);
        println!("NEW BLOCKS: {:?}", new_blocks);
        
        let result = diff_markdown(old, new);
        println!("RESULT:\n{}", result);
        assert!(result.contains("<h1>Header</h1>"));
        // The inline word diff should create <del> and <ins> within the paragraph block
        assert!(result.contains("<p>Paragraph <del>1.</del><ins>2.</ins></p>"));
        // The last block is completely different words
        assert!(result.contains("<p>Another <ins>new</ins><ins> </ins>block.</p>"));
    }
}
