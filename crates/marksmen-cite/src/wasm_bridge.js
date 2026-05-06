/**
 * wasm_bridge.js
 *
 * Unified IPC bridge for Marksmen Cite frontend.
 * Intercepts Tauri invoke calls when running in the browser.
 */

const LOCAL_REFS_KEY = 'marksmen_cite_references';
const LOCAL_COLS_KEY = 'marksmen_cite_collections';

export async function invoke(cmd, args = {}) {
    // ── Tauri path ──────────────────────────────────────────────────────────
    if (window.__TAURI__) {
        return window.__TAURI__.core.invoke(cmd, args);
    }

    // ── WASM / browser path ─────────────────────────────────────────────────
    switch (cmd) {
        case 'load_references':
            return JSON.parse(localStorage.getItem(LOCAL_REFS_KEY) || '[]');
            
        case 'save_references':
            localStorage.setItem(LOCAL_REFS_KEY, JSON.stringify(args.references || []));
            return;
            
        case 'load_collections':
            return JSON.parse(localStorage.getItem(LOCAL_COLS_KEY) || '[]');
            
        case 'save_collections':
            localStorage.setItem(LOCAL_COLS_KEY, JSON.stringify(args.collections || []));
            return;
            
        case 'format_citation': {
            try {
                // If marksmen_wasm is imported by editor, we'd use it here.
                // For now, return a basic formatted string or use marksmen-wasm if we load it.
                // marksmen-cite main.js usually needs the CSL formatter. 
                const ref = args.reference || {};
                const author = ref.author || 'Unknown';
                const year = ref.year || 'n.d.';
                return `${author.split(',')[0].trim()} (${year}). ${ref.title || ''}`;
            } catch (e) {
                return 'Format Error';
            }
        }
            
        case 'fetch_doi':
            try {
                const res = await fetch(`https://api.crossref.org/works/${encodeURIComponent(args.doi)}`);
                if (!res.ok) throw new Error('Network response was not ok');
                const data = await res.json();
                return _crossrefToRef(data.message);
            } catch (e) {
                console.error("fetch_doi failed", e);
                throw e;
            }

        case 'fetch_pmid':
            try {
                const res = await fetch(`https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id=${encodeURIComponent(args.pmid)}&retmode=json`);
                if (!res.ok) throw new Error('Network response was not ok');
                const data = await res.json();
                return _pubmedToRef(data.result[args.pmid], args.pmid);
            } catch (e) {
                console.error("fetch_pmid failed", e);
                throw e;
            }

        case 'fetch_arxiv':
            try {
                const res = await fetch(`https://export.arxiv.org/api/query?id_list=${encodeURIComponent(args.arxivId)}`);
                if (!res.ok) throw new Error('Network response was not ok');
                const text = await res.text();
                return _arxivToRef(text, args.arxivId);
            } catch (e) {
                console.error("fetch_arxiv failed", e);
                throw e;
            }

        case 'fetch_isbn':
            try {
                // Open Library API
                const res = await fetch(`https://openlibrary.org/api/books?bibkeys=ISBN:${encodeURIComponent(args.isbn)}&format=json&jscmd=data`);
                if (!res.ok) throw new Error('Network response was not ok');
                const data = await res.json();
                return _isbnToRef(data[`ISBN:${args.isbn}`], args.isbn);
            } catch (e) {
                console.error("fetch_isbn failed", e);
                throw e;
            }

        case 'import_pdf':
            console.warn('[WASM bridge] import_pdf: not supported in browser mode.');
            throw new Error("PDF import requires desktop app.");

        case 'open_pdf_native':
            if (args.path) {
                // If it's an absolute path, we can't open it in browser, but if it's a URL...
                if (args.path.startsWith('http')) window.open(args.path, '_blank');
                else console.warn('[WASM bridge] open_pdf_native: cannot open local paths in browser.');
            }
            return;

        case 'export_ris':
        case 'export_bibtex': {
            // Simplified fallback
            const blob = new Blob([JSON.stringify(args.references, null, 2)], { type: 'text/plain' });
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = `references.${cmd === 'export_ris' ? 'ris' : 'bib'}`;
            a.click();
            URL.revokeObjectURL(url);
            return;
        }

        case 'import_lib_file':
            console.warn('[WASM bridge] import_lib_file: use file picker manually in future.');
            throw new Error("Lib import requires desktop app or browser file picker.");

        default:
            console.warn(`[WASM bridge] Unhandled cite command '${cmd}' in browser mode.`);
            return null;
    }
}

// Minimal parsers to map API responses to marksmen Reference object
function _crossrefToRef(item) {
    return {
        id: item.DOI || crypto.randomUUID(),
        title: item.title?.[0] || 'Unknown Title',
        author: (item.author || []).map(a => `${a.family}, ${a.given}`).join('; '),
        year: item.issued?.['date-parts']?.[0]?.[0]?.toString() || '',
        journal: item['container-title']?.[0] || '',
        doi: item.DOI || '',
        pmid: '',
        url: item.URL || '',
        abstract_text: '',
        pdf_path: '',
        date_added: new Date().toISOString()
    };
}

function _pubmedToRef(item, pmid) {
    return {
        id: pmid || crypto.randomUUID(),
        title: item.title || 'Unknown Title',
        author: (item.authors || []).map(a => a.name).join('; '),
        year: item.pubdate ? item.pubdate.split(' ')[0] : '',
        journal: item.fulljournalname || '',
        doi: item.articleids?.find(id => id.idtype === 'doi')?.value || '',
        pmid: pmid,
        url: `https://pubmed.ncbi.nlm.nih.gov/${pmid}`,
        abstract_text: '',
        pdf_path: '',
        date_added: new Date().toISOString()
    };
}

function _arxivToRef(xmlText, arxivId) {
    // extremely naive XML extraction
    const titleMatch = xmlText.match(/<title>(.*?)<\/title>/is);
    return {
        id: arxivId || crypto.randomUUID(),
        title: titleMatch ? titleMatch[1].trim() : 'Unknown',
        author: 'ArXiv Author',
        year: new Date().getFullYear().toString(),
        journal: 'ArXiv',
        doi: '',
        pmid: '',
        url: `https://arxiv.org/abs/${arxivId}`,
        abstract_text: '',
        pdf_path: '',
        date_added: new Date().toISOString()
    };
}

function _isbnToRef(item, isbn) {
    if (!item) throw new Error("ISBN not found");
    return {
        id: isbn || crypto.randomUUID(),
        title: item.title || 'Unknown',
        author: (item.authors || []).map(a => a.name).join('; '),
        year: item.publish_date || '',
        journal: '',
        doi: '',
        pmid: '',
        url: item.url || '',
        abstract_text: '',
        pdf_path: '',
        date_added: new Date().toISOString()
    };
}
