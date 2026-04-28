use docx_rs::*;

fn main() {
    let mut doc = Docx::new();
    let p = Paragraph::new().add_footnoteReference(1);
}
