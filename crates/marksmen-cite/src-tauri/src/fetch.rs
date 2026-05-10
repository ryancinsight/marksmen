//! Web API fetchers: Crossref (DOI), PubMed (PMID), arXiv, OpenLibrary (ISBN).

use crate::model::Reference;
use regex::Regex;
use std::sync::OnceLock;

// ── Compiled regex statics ────────────────────────────────────────────────────
// All patterns are compile-time string literals. A panic here at first use is
// equivalent to a compile error and indicates a programming error, not a runtime
// condition — the OnceLock ensures each pattern is compiled exactly once.

fn re_xml_tags() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"<[^>]+>").expect("re_xml_tags: invalid pattern"))
}

fn re_arxiv_title() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"<title>([^<]+)</title>").expect("re_arxiv_title: invalid pattern")
    })
}

fn re_arxiv_author() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"<name>([^<]+)</name>").expect("re_arxiv_author: invalid pattern"))
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("Marksmen-Cite/2.0 (mailto:ryanclanton@outlook.com)")
        .build()
        .map_err(|e| e.to_string())
}

/// Strip JATS/XML tags from Crossref abstract strings.
fn strip_jats(s: &str) -> String {
    re_xml_tags().replace_all(s, "").trim().to_string()
}

// ── Crossref DOI ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fetch_doi(doi: String) -> Result<Reference, String> {
    let client = http_client()?;
    let url = format!("https://api.crossref.org/works/{}", doi.trim());
    let res = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Crossref: {}", res.status()));
    }
    let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let m = &json["message"];

    let title = m["title"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|t| t.as_str())
        .unwrap_or("Untitled")
        .to_string();

    let mut authors = Vec::new();
    if let Some(arr) = m["author"].as_array() {
        for a in arr {
            let given = a["given"].as_str().unwrap_or("");
            let family = a["family"].as_str().unwrap_or("");
            if !family.is_empty() {
                authors.push(if given.is_empty() {
                    family.to_string()
                } else {
                    format!("{}, {}", family, given)
                });
            }
        }
    }

    let abstract_text = strip_jats(m["abstract"].as_str().unwrap_or(""));
    let journal = m["container-title"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let issn = m["ISSN"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let volume = m["volume"].as_str().unwrap_or("").to_string();
    let issue = m["issue"].as_str().unwrap_or("").to_string();
    let pages = m["page"].as_str().unwrap_or("").to_string();
    let publisher = m["publisher"].as_str().unwrap_or("").to_string();
    let year = m["issued"]["date-parts"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|d| d.as_array())
        .and_then(|d| d.first())
        .and_then(|y| y.as_i64())
        .map(|y| y.to_string())
        .unwrap_or_default();
    let url = m["URL"].as_str().unwrap_or("").to_string();
    let ref_type = match m["type"].as_str().unwrap_or("journal-article") {
        "journal-article" => "Journal Article",
        "book" => "Book",
        "book-chapter" => "Book Chapter",
        "proceedings-article" => "Conference Paper",
        _ => "Journal Article",
    }
    .to_string();

    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    Ok(Reference {
        id: uuid::Uuid::new_v4().to_string(),
        reference_type: ref_type,
        title,
        authors,
        abstract_text,
        journal,
        volume,
        issue,
        pages,
        publisher,
        doi,
        issn,
        url,
        year,
        date_added: now.clone(),
        date_modified: now,
        ..Reference::blank()
    })
}

// ── PubMed PMID ──────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fetch_pmid(pmid: String) -> Result<Reference, String> {
    let client = http_client()?;
    let pmid = pmid.trim().to_string();

    // esummary → basic metadata JSON
    let summary_url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&retmode=json&id={}",
        pmid
    );
    let json: serde_json::Value = client
        .get(&summary_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let rec = &json["result"][&pmid];
    if rec.is_null() {
        return Err(format!("PubMed: no record for PMID {}", pmid));
    }

    let title = rec["title"]
        .as_str()
        .unwrap_or("Untitled")
        .trim_end_matches('.')
        .to_string();
    let journal = rec["source"].as_str().unwrap_or("").to_string();
    let volume = rec["volume"].as_str().unwrap_or("").to_string();
    let issue = rec["issue"].as_str().unwrap_or("").to_string();
    let pages = rec["pages"].as_str().unwrap_or("").to_string();
    let issn = rec["issn"].as_str().unwrap_or("").to_string();
    let year = rec["pubdate"]
        .as_str()
        .unwrap_or("")
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();

    let mut authors = Vec::new();
    if let Some(arr) = rec["authors"].as_array() {
        for a in arr {
            if a["authtype"].as_str() == Some("Author") {
                if let Some(name) = a["name"].as_str() {
                    authors.push(name.to_string());
                }
            }
        }
    }

    // Attempt to extract DOI from articleids
    let mut doi = String::new();
    if let Some(ids) = rec["articleids"].as_array() {
        for id in ids {
            if id["idtype"].as_str() == Some("doi") {
                doi = id["value"].as_str().unwrap_or("").to_string();
                break;
            }
        }
    }

    // efetch → abstract (XML, use regex extraction)
    let abstract_text = fetch_pubmed_abstract(&client, &pmid)
        .await
        .unwrap_or_default();

    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    Ok(Reference {
        id: uuid::Uuid::new_v4().to_string(),
        reference_type: "Journal Article".to_string(),
        title,
        authors,
        abstract_text,
        journal,
        volume,
        issue,
        pages,
        doi,
        pmid,
        issn,
        year,
        date_added: now.clone(),
        date_modified: now,
        ..Reference::blank()
    })
}

async fn fetch_pubmed_abstract(client: &reqwest::Client, pmid: &str) -> Option<String> {
    static RE_ABSTRACT: OnceLock<Regex> = OnceLock::new();
    let re_abstract = RE_ABSTRACT.get_or_init(|| {
        Regex::new(r"<AbstractText[^>]*>([\s\S]*?)</AbstractText>")
            .expect("re_abstract: invalid pattern")
    });

    let url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=pubmed&id={}&rettype=xml&retmode=xml",
        pmid
    );
    let text = client.get(&url).send().await.ok()?.text().await.ok()?;
    let caps = re_abstract.captures(&text)?;
    let raw = caps.get(1)?.as_str();
    // Strip any remaining XML tags within abstract sections using the shared static.
    Some(re_xml_tags().replace_all(raw, "").trim().to_string())
}

// ── arXiv ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fetch_arxiv(arxiv_id: String) -> Result<Reference, String> {
    let client = http_client()?;
    // Normalize: strip "arxiv:" prefix or URL if present
    let id = arxiv_id
        .trim()
        .trim_start_matches("https://arxiv.org/abs/")
        .trim_start_matches("arxiv:")
        .to_string();

    let url = format!("https://export.arxiv.org/api/query?id_list={}", id);
    let xml = client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    // Regex extraction from Atom feed using OnceLock statics.
    let extract = |re: &Regex| -> String {
        re.captures(&xml)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default()
    };

    // The feed-level <title> appears first; the entry title is the second match.
    let titles: Vec<&str> = re_arxiv_title()
        .captures_iter(&xml)
        .filter_map(|c| c.get(1).map(|m| m.as_str()))
        .collect();
    let title = titles
        .get(1)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| extract(re_arxiv_title()))
        .replace('\n', " ");

    // summary and year use locally constructed (one-shot) patterns — these are
    // not shared across hot loops so inline Regex::new with .ok() is appropriate.
    let abstract_text = {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(r"<summary[^>]*>([\s\S]*?)</summary>").expect("re_summary: invalid pattern")
        });
        extract(re).replace('\n', " ").trim().to_string()
    };

    let year = {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(r"<published>(\d{4})").expect("re_published: invalid pattern")
        });
        extract(re)
    };

    let arxiv_doi = {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(r#"<arxiv:doi[^>]*>([^<]+)</arxiv:doi>"#)
                .expect("re_arxiv_doi: invalid pattern")
        });
        extract(re)
    };

    let url_str = format!("https://arxiv.org/abs/{}", id);

    let mut authors = Vec::new();
    for cap in re_arxiv_author().captures_iter(&xml) {
        if let Some(m) = cap.get(1) {
            authors.push(m.as_str().trim().to_string());
        }
    }

    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    Ok(Reference {
        id: uuid::Uuid::new_v4().to_string(),
        reference_type: "Preprint".to_string(),
        title,
        authors,
        abstract_text,
        year,
        doi: arxiv_doi,
        url: url_str,
        journal: "arXiv".to_string(),
        date_added: now.clone(),
        date_modified: now,
        ..Reference::blank()
    })
}

// ── OpenLibrary ISBN ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fetch_isbn(isbn: String) -> Result<Reference, String> {
    let client = http_client()?;
    let isbn = isbn.trim().replace(['-', ' '], "");
    let url = format!(
        "https://openlibrary.org/api/books?bibkeys=ISBN:{}&format=json&jscmd=data",
        isbn
    );
    let json: serde_json::Value = client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let key = format!("ISBN:{}", isbn);
    let rec = json
        .get(&key)
        .ok_or_else(|| format!("No record found for ISBN {}", isbn))?;

    let title = rec["title"].as_str().unwrap_or("Untitled").to_string();
    let publisher = rec["publishers"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|p| p["name"].as_str())
        .unwrap_or("")
        .to_string();
    let year = rec["publish_date"]
        .as_str()
        .unwrap_or("")
        .split_whitespace()
        .last()
        .unwrap_or("")
        .to_string();

    let mut authors = Vec::new();
    if let Some(arr) = rec["authors"].as_array() {
        for a in arr {
            if let Some(name) = a["name"].as_str() {
                authors.push(name.to_string());
            }
        }
    }

    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    Ok(Reference {
        id: uuid::Uuid::new_v4().to_string(),
        reference_type: "Book".to_string(),
        title,
        authors,
        year,
        publisher,
        isbn,
        date_added: now.clone(),
        date_modified: now,
        ..Reference::blank()
    })
}
