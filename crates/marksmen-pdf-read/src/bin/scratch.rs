fn detect_bullet(text: &str) -> (bool, Option<String>, String) {
    let mut parts = text.splitn(2, ". ");
    if let (Some(num), Some(rest)) = (parts.next(), parts.next()) {
        if num.chars().all(|c| c.is_ascii_digit()) && !num.is_empty() {
            return (true, Some(format!("{}.", num)), rest.to_string());
        }
    }
    (false, None, text.to_string())
}
fn main() {
    println!("{:?}", detect_bullet("10. Yang"));
}
