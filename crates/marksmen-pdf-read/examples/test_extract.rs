use std::fs;

fn main() {
    let bytes = fs::read("../../resume/RC_CurriculumVitae_2026.pdf").unwrap();

    // Also test parse_pdf
    match marksmen_pdf_read::parse_pdf(&bytes) {
        Ok(md) => {
            println!("\n=== PARSE_PDF: {} chars ===", md.len());
            println!("{}", md);
        }
        Err(e) => {
            eprintln!("\nPARSE_PDF ERROR: {}", e);
        }
    }
}
