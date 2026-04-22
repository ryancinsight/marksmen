use docx_rs::*;
fn main() {
    let mut p = Paragraph::new().add_run(Run::new().add_text("ABCD"));
    p = p.align(AlignmentType::Center);
    println!("Has runs: {}", p.children.len());
}
