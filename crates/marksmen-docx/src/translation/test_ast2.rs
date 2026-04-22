use docx_rs::*;

pub enum Container<'a> {
    Doc(&'a mut Docx),
    Header(&'a mut Header),
    Footer(&'a mut Footer)
}

impl<'a> Container<'a> {
    pub fn add_paragraph(mut self, p: Paragraph) -> Self {
        match self {
            Self::Doc(ref mut d) => { **d = d.clone().add_paragraph(p); self }
            Self::Header(ref mut h) => { **h = h.clone().add_paragraph(p); self }
            Self::Footer(ref mut f) => { **f = f.clone().add_paragraph(p); self }
        }
    }
}
