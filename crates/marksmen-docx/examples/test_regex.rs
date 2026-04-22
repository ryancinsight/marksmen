use regex::Regex;
use std::collections::HashSet;

fn deduplicate_comments(xml: &str) -> String {
    let re = Regex::new(r"(?s)<w:comment\s+w:id="([^"]+)"[^>]*>.*?</w:comment>").unwrap();
    let mut seen = HashSet::new();
    let mut output = String::new();
    let mut last_end = 0;
    
    for cap in re.captures_iter(xml) {
        let mat = cap.get(0).unwrap();
        output.push_str(&xml[last_end..mat.start()]);
        
        let id = cap.get(1).unwrap().as_str();
        if !seen.contains(id) {
            seen.insert(id.to_string());
            output.push_str(mat.as_str());
        }
        last_end = mat.end();
    }
    
    output.push_str(&xml[last_end..]);
    output
}

fn main() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:comments><w:comment w:id="0" w:author="Ryan" w:date="1970-01-01T00:00:00Z" w:initials=""><w:p w14:paraId="00000001"><w:pPr><w:rPr /></w:pPr><w:r><w:rPr /><w:t xml:space="preserve">This is a test</w:t></w:r></w:p></w:comment><w:comment w:id="0" w:author="Ryan" w:date="1970-01-01T00:00:00Z" w:initials=""><w:p w14:paraId="00000001"><w:pPr><w:rPr /></w:pPr><w:r><w:rPr /><w:t xml:space="preserve">This is a test</w:t></w:r></w:p></w:comment></w:comments>"#;
    let dedup = deduplicate_comments(xml);
    println!("{}", dedup);
}
