//! Export commands and CSL-lite citation formatter (APA 7, MLA 9, Chicago 17, Vancouver, IEEE).

use crate::model::Reference;

// ── Author name helpers ───────────────────────────────────────────────────────

struct Author {
    last: String,
    first: String,
}

fn parse_author(s: &str) -> Author {
    if let Some(pos) = s.find(',') {
        Author {
            last: s[..pos].trim().to_string(),
            first: s[pos + 1..].trim().to_string(),
        }
    } else {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() >= 2 {
            Author {
                last: parts.last().unwrap_or(&"").to_string(),
                first: parts[..parts.len() - 1].join(" "),
            }
        } else {
            Author {
                last: s.to_string(),
                first: String::new(),
            }
        }
    }
}

fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .map(|c| format!("{}.", c))
        .collect::<Vec<_>>()
        .join(" ")
}

// ── APA 7 ─────────────────────────────────────────────────────────────────────

fn apa_authors(authors: &[String]) -> String {
    let parsed: Vec<Author> = authors.iter().map(|a| parse_author(a)).collect();
    let fmt: Vec<String> = parsed
        .iter()
        .map(|a| format!("{}, {}", a.last, initials(&a.first)))
        .collect();
    match fmt.len() {
        0 => String::new(),
        1 => fmt[0].clone(),
        2 => format!("{}, & {}", fmt[0], fmt[1]),
        3..=7 => {
            let (last, rest) = fmt.split_last().unwrap();
            format!("{}, & {}", rest.join(", "), last)
        }
        _ => {
            let first6 = fmt[..6].join(", ");
            format!("{}, ... {}", first6, fmt.last().unwrap())
        }
    }
}

fn format_apa(r: &Reference) -> String {
    let authors = apa_authors(&r.authors);
    let year = if r.year.is_empty() {
        "n.d.".to_string()
    } else {
        r.year.clone()
    };
    let doi_part = if !r.doi.is_empty() {
        format!(" https://doi.org/{}", r.doi)
    } else {
        String::new()
    };
    let vol_issue = match (r.volume.is_empty(), r.issue.is_empty()) {
        (false, false) => format!(", *{}*({})", r.volume, r.issue),
        (false, true) => format!(", *{}*", r.volume),
        _ => String::new(),
    };
    let pages = if !r.pages.is_empty() {
        format!(", {}", r.pages)
    } else {
        String::new()
    };
    let journal = if !r.journal.is_empty() {
        format!(" *{}*{}{}.{}", r.journal, vol_issue, pages, doi_part)
    } else {
        doi_part
    };
    format!("{} ({}).  {}. {}", authors, year, r.title, journal)
        .trim()
        .to_string()
}

// ── MLA 9 ─────────────────────────────────────────────────────────────────────

fn mla_authors(authors: &[String]) -> String {
    let parsed: Vec<Author> = authors.iter().map(|a| parse_author(a)).collect();
    match parsed.len() {
        0 => String::new(),
        1 => {
            let a = &parsed[0];
            if a.first.is_empty() {
                a.last.clone()
            } else {
                format!("{}, {}", a.last, a.first)
            }
        }
        _ => {
            let a = &parsed[0];
            let first = if a.first.is_empty() {
                a.last.clone()
            } else {
                format!("{}, {}", a.last, a.first)
            };
            format!("{}, et al.", first)
        }
    }
}

fn format_mla(r: &Reference) -> String {
    let authors = mla_authors(&r.authors);
    let vol = if !r.volume.is_empty() {
        format!("vol. {}, ", r.volume)
    } else {
        String::new()
    };
    let issue = if !r.issue.is_empty() {
        format!("no. {}, ", r.issue)
    } else {
        String::new()
    };
    let pages = if !r.pages.is_empty() {
        format!("pp. {}", r.pages)
    } else {
        String::new()
    };
    let doi = if !r.doi.is_empty() {
        format!(", doi:{}", r.doi)
    } else {
        String::new()
    };
    format!(
        "{} \"{}\" *{}*, {}{}{}, {}{}.",
        if authors.is_empty() {
            String::new()
        } else {
            format!("{}.", authors)
        },
        r.title,
        r.journal,
        vol,
        issue,
        r.year,
        pages,
        doi
    )
    .trim()
    .to_string()
}

// ── Chicago 17 (author-date) ──────────────────────────────────────────────────

fn chicago_authors(authors: &[String]) -> String {
    let parsed: Vec<Author> = authors.iter().map(|a| parse_author(a)).collect();
    match parsed.len() {
        0 => String::new(),
        1 => {
            let a = &parsed[0];
            if a.first.is_empty() {
                a.last.clone()
            } else {
                format!("{}, {}", a.last, a.first)
            }
        }
        _ => {
            let first = &parsed[0];
            let rest: Vec<String> = parsed[1..]
                .iter()
                .map(|a| {
                    if a.first.is_empty() {
                        a.last.clone()
                    } else {
                        format!("{} {}", a.first, a.last)
                    }
                })
                .collect();
            format!(
                "{}, {}, and {}",
                first.last,
                first.first,
                rest.join(", and ")
            )
        }
    }
}

fn format_chicago(r: &Reference) -> String {
    let authors = chicago_authors(&r.authors);
    let vol_issue = match (r.volume.is_empty(), r.issue.is_empty()) {
        (false, false) => format!(" {} ({})", r.volume, r.issue),
        (false, true) => format!(" {}", r.volume),
        _ => String::new(),
    };
    let pages = if !r.pages.is_empty() {
        format!(": {}", r.pages)
    } else {
        String::new()
    };
    let doi = if !r.doi.is_empty() {
        format!(" https://doi.org/{}", r.doi)
    } else {
        String::new()
    };
    format!(
        "{} {} \"{}\" *{}*{}{}.{}",
        authors, r.year, r.title, r.journal, vol_issue, pages, doi
    )
    .trim()
    .to_string()
}

// ── Vancouver ─────────────────────────────────────────────────────────────────

fn format_vancouver(r: &Reference) -> String {
    let authors: Vec<String> = r
        .authors
        .iter()
        .map(|a| {
            let p = parse_author(a);
            format!("{} {}", p.last, initials(&p.first).replace(['.', ' '], ""))
        })
        .collect();
    let author_str = if authors.len() > 6 {
        format!("{}, et al.", authors[..6].join(", "))
    } else {
        authors.join(", ")
    };
    let vol_issue = match (r.volume.is_empty(), r.issue.is_empty()) {
        (false, false) => format!(
            "{};{}({})",
            if !r.year.is_empty() { &r.year } else { "" },
            r.volume,
            r.issue
        ),
        (false, true) => format!(
            "{};{}",
            if !r.year.is_empty() { &r.year } else { "" },
            r.volume
        ),
        _ => r.year.clone(),
    };
    let pages = if !r.pages.is_empty() {
        format!(":{}", r.pages)
    } else {
        String::new()
    };
    format!(
        "{}. {}. {}. {}{}.",
        author_str, r.title, r.journal, vol_issue, pages
    )
    .trim()
    .to_string()
}

// ── IEEE ──────────────────────────────────────────────────────────────────────

fn format_ieee(r: &Reference) -> String {
    let authors: Vec<String> = r
        .authors
        .iter()
        .map(|a| {
            let p = parse_author(a);
            format!("{} {}", initials(&p.first), p.last)
        })
        .collect();
    let author_str = match authors.len() {
        0 => String::new(),
        1 => authors[0].clone(),
        2 => format!("{} and {}", authors[0], authors[1]),
        _ => format!("{} et al.", authors[0]),
    };
    let vol = if !r.volume.is_empty() {
        format!(", vol. {}", r.volume)
    } else {
        String::new()
    };
    let no = if !r.issue.is_empty() {
        format!(", no. {}", r.issue)
    } else {
        String::new()
    };
    let pp = if !r.pages.is_empty() {
        format!(", pp. {}", r.pages)
    } else {
        String::new()
    };
    let doi = if !r.doi.is_empty() {
        format!(", doi: {}", r.doi)
    } else {
        String::new()
    };
    format!(
        "{}, \"{}\", *{}*{}{}{}, {}{}",
        author_str, r.title, r.journal, vol, no, pp, r.year, doi
    )
    .trim()
    .to_string()
}

// ── Public command ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn format_citation(reference: Reference, style: String) -> Result<String, String> {
    Ok(match style.to_lowercase().as_str() {
        "apa" => format_apa(&reference),
        "mla" => format_mla(&reference),
        "chicago" => format_chicago(&reference),
        "vancouver" => format_vancouver(&reference),
        "ieee" => format_ieee(&reference),
        other => return Err(format!("Unknown citation style: {}", other)),
    })
}

// ── RIS Export ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn export_ris(references: Vec<Reference>) -> Result<String, String> {
    let mut out = String::new();
    for r in &references {
        let ty = match r.reference_type.as_str() {
            "Journal Article" => "JOUR",
            "Book" => "BOOK",
            "Book Chapter" => "CHAP",
            "Conference Paper" => "CONF",
            "Thesis" => "THES",
            "Report" => "RPRT",
            "Website" => "ELEC",
            "Preprint" => "JOUR",
            _ => "GEN",
        };
        out.push_str(&format!("TY  - {}\n", ty));
        out.push_str(&format!("T1  - {}\n", r.title));
        for a in &r.authors {
            out.push_str(&format!("AU  - {}\n", a));
        }
        if !r.abstract_text.is_empty() {
            out.push_str(&format!("AB  - {}\n", r.abstract_text));
        }
        if !r.journal.is_empty() {
            out.push_str(&format!("JO  - {}\n", r.journal));
        }
        if !r.year.is_empty() {
            out.push_str(&format!("PY  - {}\n", r.year));
        }
        if !r.volume.is_empty() {
            out.push_str(&format!("VL  - {}\n", r.volume));
        }
        if !r.issue.is_empty() {
            out.push_str(&format!("IS  - {}\n", r.issue));
        }
        if !r.pages.is_empty() {
            out.push_str(&format!("SP  - {}\n", r.pages));
        }
        if !r.publisher.is_empty() {
            out.push_str(&format!("PB  - {}\n", r.publisher));
        }
        if !r.doi.is_empty() {
            out.push_str(&format!("DO  - {}\n", r.doi));
        }
        if !r.url.is_empty() {
            out.push_str(&format!("UR  - {}\n", r.url));
        }
        if !r.issn.is_empty() {
            out.push_str(&format!("SN  - {}\n", r.issn));
        }
        if !r.isbn.is_empty() {
            out.push_str(&format!("SN  - {}\n", r.isbn));
        }
        if !r.language.is_empty() {
            out.push_str(&format!("LA  - {}\n", r.language));
        }
        for kw in &r.tags {
            out.push_str(&format!("KW  - {}\n", kw));
        }
        if !r.notes.is_empty() {
            out.push_str(&format!("N1  - {}\n", r.notes));
        }
        out.push_str("ER  - \n\n");
    }
    Ok(out)
}

// ── BibTeX Export ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn export_bibtex(references: Vec<Reference>) -> Result<String, String> {
    let mut out = String::new();
    for r in &references {
        let entry_type = match r.reference_type.as_str() {
            "Journal Article" => "article",
            "Book" => "book",
            "Book Chapter" => "incollection",
            "Conference Paper" => "inproceedings",
            "Thesis" => "phdthesis",
            "Report" => "techreport",
            _ => "misc",
        };
        // Build a key from first author last name + year
        let key_author = r
            .authors
            .first()
            .map(|a| {
                a.split(',')
                    .next()
                    .unwrap_or("Unknown")
                    .trim()
                    .replace(' ', "")
            })
            .unwrap_or_else(|| "Unknown".to_string());
        let key = format!(
            "{}{}",
            key_author
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>(),
            r.year
        );
        out.push_str(&format!("@{}{{{},\n", entry_type, key));
        out.push_str(&format!("  title     = {{{}}},\n", r.title));
        let authors_bib = r.authors.join(" and ");
        if !authors_bib.is_empty() {
            out.push_str(&format!("  author    = {{{}}},\n", authors_bib));
        }
        if !r.journal.is_empty() {
            out.push_str(&format!("  journal   = {{{}}},\n", r.journal));
        }
        if !r.year.is_empty() {
            out.push_str(&format!("  year      = {{{}}},\n", r.year));
        }
        if !r.volume.is_empty() {
            out.push_str(&format!("  volume    = {{{}}},\n", r.volume));
        }
        if !r.issue.is_empty() {
            out.push_str(&format!("  number    = {{{}}},\n", r.issue));
        }
        if !r.pages.is_empty() {
            out.push_str(&format!(
                "  pages     = {{{}}},\n",
                r.pages.replace('-', "--")
            ));
        }
        if !r.publisher.is_empty() {
            out.push_str(&format!("  publisher = {{{}}},\n", r.publisher));
        }
        if !r.doi.is_empty() {
            out.push_str(&format!("  doi       = {{{}}},\n", r.doi));
        }
        if !r.isbn.is_empty() {
            out.push_str(&format!("  isbn      = {{{}}},\n", r.isbn));
        }
        if !r.abstract_text.is_empty() {
            out.push_str(&format!("  abstract  = {{{}}},\n", r.abstract_text));
        }
        if !r.tags.is_empty() {
            out.push_str(&format!("  keywords  = {{{}}},\n", r.tags.join(", ")));
        }
        out.push_str("}\n\n");
    }
    Ok(out)
}
