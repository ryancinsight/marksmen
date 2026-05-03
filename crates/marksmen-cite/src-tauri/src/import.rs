//! File importers: PDF (with DOI fallback), RIS, BibTeX.

use crate::fetch::fetch_doi;
use crate::model::Reference;
use marksmen_pdf_read::extract_pdf_metadata;
use regex::Regex;

// ── PDF ───────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn open_pdf_native(path: String) -> Result<(), String> {
    let url = format!("file://{}", path.replace('\\', "/"));
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_pdf(app: tauri::AppHandle) -> Result<Option<Reference>, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = match app.dialog().file()
        .add_filter("PDF Documents", &["pdf"])
        .blocking_pick_file()
    {
        Some(p) => p.into_path().map_err(|e| e.to_string())?,
        None => return Ok(None),
    };

    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let mut r = Reference {
        id: uuid::Uuid::new_v4().to_string(),
        title: "Untitled".to_string(),
        pdf_path: Some(path.to_string_lossy().into_owned()),
        date_added: now.clone(),
        date_modified: now,
        ..Reference::blank()
    };

    // Try DOI extraction from text first (most reliable for academic papers)
    if let Ok(text) = marksmen_pdf_read::parse_pdf(&bytes) {
        if let Ok(re) = Regex::new(r"(?i)10\.\d{4,9}/[-._;()/:A-Z0-9]+") {
            if let Some(mat) = re.find(&text) {
                let doi = mat.as_str().trim_end_matches(&['.', ',', ';', ')'][..]).to_string();
                if let Ok(mut fetched) = fetch_doi(doi).await {
                    fetched.id = r.id.clone();
                    fetched.pdf_path = r.pdf_path.clone();
                    fetched.date_added = r.date_added.clone();
                    return Ok(Some(fetched));
                }
            }
        }
    }

    // Fallback: PDF Info dictionary metadata
    if let Ok(meta) = extract_pdf_metadata(&bytes) {
        if let Some(t) = meta.title.filter(|t| !t.is_empty()) { r.title = t; }
        if let Some(a) = meta.author.filter(|a| !a.is_empty()) {
            r.authors = a.split([',', ';'])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }

    Ok(Some(r))
}

// ── RIS ───────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn import_ris(content: String) -> Result<Vec<Reference>, String> {
    let mut refs = Vec::new();
    let mut cur: Option<Reference> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }

        // RIS tag format: "XX  - value" (tag is first 2 chars, then 2 spaces, dash, space)
        if line.len() > 6 && &line[2..6] == "  - " {
            let tag = &line[0..2];
            let val = line[6..].trim();

            match tag {
                "TY" => {
                    if let Some(r) = cur.take() { refs.push(r); }
                    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
                    let ref_type = match val {
                        "JOUR" => "Journal Article",
                        "BOOK" => "Book",
                        "CHAP" => "Book Chapter",
                        "CONF" | "CPAPER" => "Conference Paper",
                        "THES" => "Thesis",
                        "RPRT" => "Report",
                        "ELEC" => "Website",
                        _ => "Journal Article",
                    };
                    cur = Some(Reference {
                        id: uuid::Uuid::new_v4().to_string(),
                        reference_type: ref_type.to_string(),
                        date_added: now.clone(),
                        date_modified: now,
                        ..Reference::blank()
                    });
                }
                "ER" => {
                    if let Some(r) = cur.take() { refs.push(r); }
                }
                _ => {
                    if let Some(r) = cur.as_mut() {
                        match tag {
                            "T1" | "TI" => r.title = val.to_string(),
                            "AU" | "A1" => r.authors.push(val.to_string()),
                            "AB"        => r.abstract_text = val.to_string(),
                            "JO" | "JF" | "T2" | "J2" => r.journal = val.to_string(),
                            "DO"        => r.doi = val.to_string(),
                            "PY" | "Y1" => r.year = val.chars().take(4).collect(),
                            "VL"        => r.volume = val.to_string(),
                            "IS"        => r.issue = val.to_string(),
                            "SP"        => {
                                if r.pages.is_empty() { r.pages = val.to_string(); }
                                else { r.pages = format!("{}-{}", val, r.pages); }
                            }
                            "EP"        => {
                                if r.pages.is_empty() { r.pages = val.to_string(); }
                                else { r.pages = format!("{}-{}", r.pages, val); }
                            }
                            "PB"        => r.publisher = val.to_string(),
                            "SN"        => {
                                if val.len() == 13 || val.len() == 10 { r.isbn = val.to_string(); }
                                else { r.issn = val.to_string(); }
                            }
                            "UR"        => r.url = val.to_string(),
                            "LA"        => r.language = val.to_string(),
                            "KW"        => r.tags.push(val.to_string()),
                            "N1" | "N2" => r.notes = val.to_string(),
                            _           => {}
                        }
                    }
                }
            }
        }
    }
    if let Some(r) = cur { refs.push(r); }
    Ok(refs)
}

// ── BibTeX ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn import_bibtex(content: String) -> Result<Vec<Reference>, String> {
    let mut refs = Vec::new();
    for entry in content.split('@').skip(1) {
        let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
        // Determine type from entry header
        let entry_type = entry.split(['{', '('])
            .next().unwrap_or("").trim().to_lowercase();
        let ref_type = match entry_type.as_str() {
            "article"       => "Journal Article",
            "book"          => "Book",
            "incollection"  => "Book Chapter",
            "inproceedings" | "conference" => "Conference Paper",
            "phdthesis" | "mastersthesis" => "Thesis",
            "techreport"    => "Report",
            "misc"          => "Other",
            _               => "Journal Article",
        };
        let mut r = Reference {
            id: uuid::Uuid::new_v4().to_string(),
            reference_type: ref_type.to_string(),
            date_added: now.clone(),
            date_modified: now,
            ..Reference::blank()
        };

        for line in entry.lines() {
            let line = line.trim();
            if let Some(eq) = line.find('=') {
                let key = line[..eq].trim().to_lowercase();
                let raw = line[eq + 1..].trim();
                // Strip surrounding braces/quotes and trailing comma
                let val = strip_bibtex_val(raw);
                match key.as_str() {
                    "title"     => r.title = val.replace(['{', '}'], ""),
                    "author"    => r.authors = val.split(" and ")
                        .map(|s| s.trim().to_string()).collect(),
                    "journal" | "booktitle" => r.journal = val.replace(['{', '}'], ""),
                    "year"      => r.year = val.to_string(),
                    "volume"    => r.volume = val.to_string(),
                    "number"    => r.issue = val.to_string(),
                    "pages"     => r.pages = val.replace("--", "-"),
                    "publisher" => r.publisher = val.replace(['{', '}'], ""),
                    "doi"       => r.doi = val.to_string(),
                    "isbn"      => r.isbn = val.to_string(),
                    "issn"      => r.issn = val.to_string(),
                    "url"       => r.url = val.to_string(),
                    "abstract"  => r.abstract_text = val.replace(['{', '}'], ""),
                    "keywords"  => r.tags = val.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()).collect(),
                    "note"      => r.notes = val.to_string(),
                    "language"  => r.language = val.to_string(),
                    _           => {}
                }
            }
        }
        if !r.title.is_empty() || !r.authors.is_empty() {
            refs.push(r);
        }
    }
    Ok(refs)
}

fn strip_bibtex_val(raw: &str) -> &str {
    let s = raw.trim_end_matches(',').trim();
    if (s.starts_with('{') && s.ends_with('}'))
        || (s.starts_with('"') && s.ends_with('"'))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

// ── File picker dispatcher ────────────────────────────────────────────────────

#[tauri::command]
pub async fn import_lib_file(app: tauri::AppHandle) -> Result<Vec<Reference>, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = match app.dialog().file()
        .add_filter("Citation Databases", &["ris", "bib"])
        .blocking_pick_file()
    {
        Some(p) => p.into_path().map_err(|e| e.to_string())?,
        None => return Ok(Vec::new()),
    };
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let ext = path.extension().unwrap_or_default().to_string_lossy().to_lowercase();
    match ext.as_str() {
        "ris" => import_ris(content).await,
        "bib" => import_bibtex(content).await,
        other => Err(format!("Unsupported file type: {}", other)),
    }
}
