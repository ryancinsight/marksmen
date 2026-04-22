use docx_rs::*;

fn main() {
    let mut h = Header::new();
    h = h.add_paragraph(Paragraph::new());
    h = h.add_table(Table::new(vec![]));
}
