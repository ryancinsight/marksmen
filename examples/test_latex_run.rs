fn main() {
    let md = "# Hello\nTest paragraph with *italic* and **bold**.\n\n- item 1\n- item 2\n\n```python\nprint(1)\n```";
    let events = marksmen_core::parsing::parser::parse(md);
    let config = marksmen_core::Config::default();
    let latex = marksmen_latex::convert(events, &config).unwrap();
    println!("{}", latex);
}
