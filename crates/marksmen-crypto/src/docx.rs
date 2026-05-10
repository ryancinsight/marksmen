use ms_offcrypto_writer::Ecma376AgileWriter;

/// Encrypts an Office Open XML (DOCX/XLSX/PPTX) stream using Agile Encryption (AES-256).
/// Returns the resulting OLE2-wrapped byte array.
pub fn protect_docx<R: std::io::Read, W: std::io::Write + std::io::Seek + std::io::Read>(
    mut input: R,
    output: W,
    password: &str,
) -> std::io::Result<()> {
    let mut rng = rand::rng();

    // Initialize the Agile Encryption writer over the provided output stream
    let mut writer = Ecma376AgileWriter::create(&mut rng, password, output)?;

    // Stream bytes through the cipher
    std::io::copy(&mut input, &mut writer)?;

    // Finalize the OLE2 wrapping
    writer.finalize()?;

    Ok(())
}
