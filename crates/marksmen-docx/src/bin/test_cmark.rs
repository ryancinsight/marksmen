use pulldown_cmark::{Event, Parser};

fn main() {
    let md = "<header>\n\n# Confidential\n\n</header>";
    let parser = Parser::new(md);
    for event in parser {
        println!("{:?}", event);
    }
}
