pub mod docx;
pub mod pdf_enc;
pub mod pkcs7;

// Stage 3.2: PDF encryption and eSignatures will go here
pub use docx::protect_docx;
pub use pdf_enc::encrypt_pdf;
pub use pkcs7::PdfSigner;
