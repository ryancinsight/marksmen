fn main() {
    let md = "<!-- page:12240x15840 margin:1440,1440,1440,1440 -->\n\n# Hello";
    let opts = pulldown_cmark::Options::all();
    let parser = pulldown_cmark::Parser::new_ext(md, opts);
    for event in parser {
        println!("{:?}", event);
    }
}
