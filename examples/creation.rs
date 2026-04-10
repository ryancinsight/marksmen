//! Demonstrates how to parse a raw Markdown string into the mathematical `Event` AST
//! utilized by the `marksmen` compilation engines.

use anyhow::Result;
use marksmen_core::config::frontmatter::parse_frontmatter;
use marksmen_core::parsing::parser::parse;

fn main() -> Result<()> {
    let markdown_source = r#"---
title: Minimal File Creation Demo
author: Ryan Clanton
---
## 1. Introduction
This demonstrates programmatic file assimilation and **AST parsing** into discrete topologies.
"#;

    println!("=== Source Markdown ===");
    println!("{}", markdown_source);

    // 1. Extract YAML Frontmatter Configuration
    let (body, frontmatter) = parse_frontmatter(markdown_source)?;
    println!("\n=== Frontmatter Extracted ===");
    println!("{:#?}", frontmatter);

    // 2. Parse Markdown Body into Event Stream
    let events = parse(body);
    println!("\n=== Abstract Syntax Tree (AST) Streams ===");
    for event in events.iter().take(5) {
        println!("{:?}", event);
    }
    println!("... ({} total parsed elements)", events.len());

    Ok(())
}
