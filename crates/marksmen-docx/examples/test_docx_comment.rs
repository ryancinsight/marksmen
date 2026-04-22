use docx_rs::*;

fn main() -> Result<(), docx_rs::ReaderError> {
    let comment = Comment::new(0).author("Ryan").add_paragraph(Paragraph::new().add_run(Run::new().add_text("This is a test")));
    
    let nested_p = Paragraph::new()
        .add_comment_start(comment)
        .add_run(Run::new().add_text("Test Comment"))
        .add_comment_end(0);
        
    let nested_cell = TableCell::new().add_paragraph(nested_p);
    let nested_row = TableRow::new(vec![nested_cell]);
    let nested_tbl = Table::new(vec![nested_row]);
    
    let outer_cell = TableCell::new().add_table(nested_tbl);
    let outer_row = TableRow::new(vec![outer_cell]);
    let outer_tbl = Table::new(vec![outer_row]);
    
    let mut doc = Docx::new().add_table(outer_tbl);
    
    let file = std::fs::File::create("test_docx.docx").unwrap();
    doc.build().pack(file).unwrap();
    Ok(())
}
