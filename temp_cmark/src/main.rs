use pulldown_cmark::{Parser, Event};
fn main() {
    let md = "Row 1: | :--- |\nRow 2: | <table class=\"nested\"><tr><td><strong>Task</strong></td><td><mark class=\"comment\" data-author=\"L\" data-content=\"C\"><mark class=\"align-center\"><strong>Status</strong></mark></mark></td></tr></table> |";
    let parser = Parser::new(md);
    for ev in parser {
        println!("{:?}", ev);
    }
}
