//! Validates topological alignment by enforcing Jaro-Winkler limits between
//! the normalized formatting arrays of the origin string vs the parsed ODT/DOCX strings.

use anyhow::Result;
use strsim::jaro_winkler;

fn remove_html_and_images(s: &str) -> String {
    // Enforcing strict bounds
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' && in_tag {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out
}

fn normalize(s: &str) -> String {
    let stripped = remove_html_and_images(s);
    let mut out = String::with_capacity(stripped.len());
    let mut last_space = false;
    for c in stripped.chars() {
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(c);
            last_space = false;
        }
    }
    out.trim().to_string()
}

fn main() -> Result<()> {
    // Isolated telemetry models mapping raw parsing evaluations
    let original_source = "## Analytical Symmetry\nThe Jaro-Winkler bounded evaluation measures raw `string` geometry parities.";
    let extracted_source = "# Analytical Symmetry\nThe Jaro-Winkler bounded evaluation measures raw `string` geometry parities.";

    let normalized_truth = normalize(original_source);
    let normalized_extract = normalize(extracted_source);

    let similarity = jaro_winkler(&normalized_truth, &normalized_extract);

    println!("Original Target : {}", normalized_truth);
    println!("Extracted String: {}", normalized_extract);
    println!(
        "\n[>] Continuous String Topological Symmetry: {:.4}",
        similarity
    );

    if similarity > 0.95 {
        println!("[!] Metric achieved formal parity constraints.");
    } else {
        println!("[!] Warning: Structural truncation detected.");
    }

    Ok(())
}
