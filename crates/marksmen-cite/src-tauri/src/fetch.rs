//! Web API fetchers: Crossref (DOI), PubMed (PMID), arXiv, OpenLibrary (ISBN).

use crate::model::Reference;
use regex::Regex;

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("Marksmen-Cite/2.0 (mailto:ryanclanton@outlook.com)")
        .build()
        .map_err(|e| e.to_string())
}

/// Strip JATS/XML tags from Crossref abstract strings.
fn strip_jats(s: &str) -> String {
    let re = Regex::new(r"<[^>]+>").unwrap();
    re.replace_all(s, "").trim().to_string()
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

    let title = m["title"].as_array()
        .and_then(|a| a.first()).and_then(|t| t.as_str())
        .unwrap_or("Untitled").to_string();

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
    let journal = m["container-title"].as_array()
        .and_then(|a| a.first()).and_then(|t| t.as_str())
        .unwrap_or("").to_string();
    let issn = m["ISSN"].as_array()
        .and_then(|a| a.first()).and_then(|t| t.as_str())
        .unwrap_or("").to_string();
    let volume = m["volume"].as_str().unwrap_or("").to_string();
    let issue  = m["issue"].as_str().unwrap_or("").to_string();
    let pages  = m["page"].as_str().unwrap_or("").to_string();
    let publisher = m["publisher"].as_str().unwrap_or("").to_string();
    let year = m["issued"]["date-parts"].as_array()
        .and_then(|a| a.first()).and_then(|d| d.as_array())
        .and_then(|d| d.first()).and_then(|y| y.as_i64())
        .map(|y| y.to_string()).unwrap_or_default();
    let url = m["URL"].as_str().unwrap_or("").to_string();
    let ref_type = match m["type"].as_str().unwrap_or("journal-article") {
        "journal-article" => "Journal Article",
        "book"            => "Book",
        "book-chapter"    => "Book Chapter",
        "proceedings-article" => "Conference Paper",
        _                 => "Journal Article",
    }.to_string();

    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    Ok(Reference {
        id: uuid::Uuid::new_v4().to_string(),
        reference_type: ref_type,
        title, authors, abstract_text, journal, volume, issue, pages,
        publisher, doi, issn, url, year,
        date_added: now.clone(), date_modified: now,
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
    let json: serde_json::Value = client.get(&summary_url).send().await
        .map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
    let rec = &json["result"][&pmid];
    if rec.is_null() {
        return Err(format!("PubMed: no record for PMID {}", pmid));
    }

    let title = rec["title"].as_str().unwrap_or("Untitled")
        .trim_end_matches('.').to_string();
    let journal = rec["source"].as_str().unwrap_or("").to_string();
    let volume  = rec["volume"].as_str().unwrap_or("").to_string();
    let issue   = rec["issue"].as_str().unwrap_or("").to_string();
    let pages   = rec["pages"].as_str().unwrap_or("").to_string();
    let issn    = rec["issn"].as_str().unwrap_or("").to_string();
    let year    = rec["pubdate"].as_str().unwrap_or("")
        .split_whitespace().next().unwrap_or("").to_string();

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
    let abstract_text = fetch_pubmed_abstract(&client, &pmid).await.unwrap_or_default();

    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    Ok(Reference {
        id: uuid::Uuid::new_v4().to_string(),
        reference_type: "Journal Article".to_string(),
        title, authors, abstract_text, journal, volume, issue, pages, doi,
        pmid, issn, year,
        date_added: now.clone(), date_modified: now,
        ..Reference::blank()
    })
}

async fn fetch_pubmed_abstract(client: &reqwest::Client, pmid: &str) -> Option<String> {
    let url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=pubmed&id={}&rettype=xml&retmode=xml",
        pmid
    );
    let text = client.get(&url).send().await.ok()?.text().await.ok()?;
    let re = Regex::new(r"<AbstractText[^>]*>([\s\S]*?)</AbstractText>").ok()?;
    let caps = re.captures(&text)?;
    let raw = caps.get(1)?.as_str();
    // Strip any remaining XML tags within abstract sections
    let clean = Regex::new(r"<[^>]+>").ok()?;
    Some(clean.replace_all(raw, "").trim().to_string())
}

// ── arXiv ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fetch_arxiv(arxiv_id: String) -> Result<Reference, String> {
    let client = http_client()?;
    // Normalize: strip "arxiv:" prefix or URL if present
    let id = arxiv_id.trim()
        .trim_start_matches("https://arxiv.org/abs/")
        .trim_start_matches("arxiv:")
        .to_string();

    let url = format!("https://export.arxiv.org/api/query?id_list={}", id);
    let xml = client.get(&url).send().await.map_err(|e| e.to_string())?
        .text().await.map_err(|e| e.to_string())?;

    // Regex extraction from Atom feed
    let extract = |pattern: &str| -> String {
        Regex::new(pattern).ok()
            .and_then(|re| re.captures(&xml))
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default()
    };

    let title = extract(r"<title>([^<]+)</title>");
    // Skip feed-level title (first match); use the entry title
    let titles: Vec<&str> = {
        let re = Regex::new(r"<title>([^<]+)</title>").unwrap();
        re.captures_iter(&xml).filter_map(|c| c.get(1).map(|m| m.as_str())).collect()
    };
    let title = titles.get(1).map(|s| s.trim().to_string())
        .unwrap_or(title).replace('\n', " ");

    let abstract_text = extract(r"<summary[^>]*>([\s\S]*?)</summary>")
        .replace('\n', " ").trim().to_string();

    let year = extract(r"<published>(\d{4})");

    let arxiv_doi = extract(r#"<arxiv:doi[^>]*>([^<]+)</arxiv:doi>"#);
    let url_str = format!("https://arxiv.org/abs/{}", id);

    let mut authors = Vec::new();
    let author_re = Regex::new(r"<name>([^<]+)</name>").unwrap();
    for cap in author_re.captures_iter(&xml) {
        if let Some(m) = cap.get(1) {
            authors.push(m.as_str().trim().to_string());
        }
    }

    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    Ok(Reference {
        id: uuid::Uuid::new_v4().to_string(),
        reference_type: "Preprint".to_string(),
        title, authors, abstract_text, year,
        doi: arxiv_doi, url: url_str,
        journal: "arXiv".to_string(),
        date_added: now.clone(), date_modified: now,
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
    let json: serde_json::Value = client.get(&url).send().await
        .map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;

    let key = format!("ISBN:{}", isbn);
    let rec = json.get(&key).ok_or_else(|| format!("No record found for ISBN {}", isbn))?;

    let title = rec["title"].as_str().unwrap_or("Untitled").to_string();
    let publisher = rec["publishers"].as_array()
        .and_then(|a| a.first()).and_then(|p| p["name"].as_str())
        .unwrap_or("").to_string();
    let year = rec["publish_date"].as_str().unwrap_or("")
        .split_whitespace().last().unwrap_or("").to_string();

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
        title, authors, year, publisher, isbn,
        date_added: now.clone(), date_modified: now,
        ..Reference::blank()
    })
}
