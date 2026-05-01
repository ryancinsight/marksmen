use docx_rs::*;

pub enum Container<'a> {
    Header(&'a mut Header),
}

impl<'a> Container<'a> {
    pub fn add_paragraph(mut self, p: Paragraph) -> Self {
        match self {
            Self::Header(ref mut h) => {
                **h = h.clone().add_paragraph(p);
                self
            }
        }
    }
}

pub fn handle_event<'a>(container: Container<'a>) {
    let p = Paragraph::new().add_run(Run::new().add_text("Working!"));
    container.add_paragraph(p);
}

fn main() {
    let mut header_buf = Header::new();
    handle_event(Container::Header(&mut header_buf));
    println!("Header length: {}", header_buf.children.len());
}
