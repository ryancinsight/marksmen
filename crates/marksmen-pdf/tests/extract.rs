fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{}=\"", attr);
    if let Some(start) = tag.find(&needle) {
        if let Some(end) = tag[start + needle.len()..].find('"') {
            return Some(tag[start + needle.len()..start + needle.len() + end].to_string());
        }
    }
    None
}
fn main() {
    println!(
        "{:?}",
        extract_attr(
            "<mark class=\"comment\" data-author=\"Test\" data-content=\"Hello\">",
            "data-author"
        )
    );
}
