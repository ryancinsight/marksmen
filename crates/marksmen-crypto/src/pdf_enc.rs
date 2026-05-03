use aes::Aes256;
use cbc::Encryptor;
use cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use lopdf::{Dictionary, Document, Object};
use rand::RngCore;
use std::io::{Read, Write};

type Aes256CbcEnc = Encryptor<Aes256>;

/// Encrypts a PDF Document using AES-256 CBC.
/// Note: This applies AES-256 to all string and stream objects directly
/// and constructs a simplified Encrypt dictionary. For full ISO 32000-2
/// compliance, proper SASLPrep and complex KDF logic must be built around the Owner/User passwords.
pub fn encrypt_pdf<R: Read, W: Write>(
    mut input: R,
    mut output: W,
    user_pw: &str,
    owner_pw: &str,
) -> anyhow::Result<()> {
    let mut doc = Document::load_from(&mut input).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // 1. Generate a random 32-byte encryption key for the document
    let mut rng = rand::rng();
    let mut file_key = [0u8; 32];
    rng.fill_bytes(&mut file_key);

    // 2. Iterate through all objects and encrypt Strings and Streams
    // (In PDF, boolean/numeric/name objects are not encrypted, only strings and streams)
    for (_id, object) in doc.objects.iter_mut() {
        // According to ISO 32000, we skip encrypting the Encrypt dictionary itself
        // But since we haven't added it yet, we just encrypt all current strings/streams.
        match object {
            Object::String(data, _) => {
                let enc = encrypt_bytes(data, &file_key, &mut rng);
                *data = enc;
            }
            Object::Stream(stream) => {
                // Decompress, encrypt, then we leave it as encrypted
                if let Ok(content) = stream.decompressed_content() {
                    let enc = encrypt_bytes(&content, &file_key, &mut rng);
                    stream.set_content(enc);
                    // Remove compression filters since we are encrypting raw content
                    stream.dict.remove(b"Filter");
                } else {
                    // Fallback to raw encryption
                    let enc = encrypt_bytes(&stream.content, &file_key, &mut rng);
                    stream.content = enc;
                }
            }
            _ => {}
        }
    }

    // 3. Construct the /Encrypt dictionary (Revision 5/6 stub for AES-256)
    let mut encrypt_dict = Dictionary::new();
    encrypt_dict.set("Filter", Object::Name(b"Standard".to_vec()));
    encrypt_dict.set("V", Object::Integer(5)); // AES-256
    encrypt_dict.set("R", Object::Integer(6));
    encrypt_dict.set("Length", Object::Integer(256));
    
    // In a real Revision 6 implementation, /O, /U, /OE, /UE, /Perms would be calculated
    // using the user_pw and owner_pw. Here we output the structural dictionary.
    encrypt_dict.set("O", Object::String(owner_pw.as_bytes().to_vec(), lopdf::StringFormat::Literal));
    encrypt_dict.set("U", Object::String(user_pw.as_bytes().to_vec(), lopdf::StringFormat::Literal));
    
    let encrypt_id = doc.add_object(Object::Dictionary(encrypt_dict));
    doc.trailer.set("Encrypt", Object::Reference(encrypt_id));

    // 4. Save the document
    doc.save_to(&mut output).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(())
}

fn encrypt_bytes(data: &[u8], key: &[u8; 32], rng: &mut impl RngCore) -> Vec<u8> {
    let mut iv = [0u8; 16];
    rng.fill_bytes(&mut iv);
    
    let encryptor = Aes256CbcEnc::new(key.into(), &iv.into());
    let mut out = encryptor.encrypt_padded_vec_mut::<Pkcs7>(data);
    
    // Prepend IV to the encrypted data
    let mut final_data = Vec::with_capacity(iv.len() + out.len());
    final_data.extend_from_slice(&iv);
    final_data.append(&mut out);
    final_data
}
