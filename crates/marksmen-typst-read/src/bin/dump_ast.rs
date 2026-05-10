fn main() {
    let src = r#"
#image("myimage.png", alt: "My Image")
"#;
    let md = marksmen_typst_read::parser::parse_typst(src).unwrap();
    println!("MARKDOWN:\n{}", md);
}
