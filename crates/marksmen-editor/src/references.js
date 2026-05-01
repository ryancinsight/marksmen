// ==========================================
// Phase 27: References Tab (G-H33–G-H38, G-M11, G-M02, G-M20)
// ==========================================

// ── Citation Source Store (G-H34 & Marksmen-Cite Integration) ───────────────
const SOURCES_KEY = 'marksmen_sources';
window.marksmenSources = [];

const { invoke } = window.__TAURI__.core;

async function loadSourcesFromCite() {
    try {
        const json = await invoke('load_marksmen_cite_db');
        const dbSources = JSON.parse(json || '[]');
        // Map marksmen-cite schema to expected APA schema
        window.marksmenSources = dbSources.map(citeRef => {
            return {
                id: citeRef.id,
                author: (citeRef.authors || []).join(', '),
                year: citeRef.year || '',
                title: citeRef.title || 'Untitled',
                publisher: citeRef.journal || '',
                type: 'article',
                url: citeRef.doi ? `https://doi.org/${citeRef.doi}` : ''
            };
        });
        
        // Merge any legacy local sources
        const localSources = JSON.parse(localStorage.getItem(SOURCES_KEY) || '[]');
        const merged = [...window.marksmenSources];
        localSources.forEach(ls => {
            if (!merged.find(m => m.id === ls.id)) {
                merged.push(ls);
            }
        });
        window.marksmenSources = merged;
    } catch(e) {
        console.error("Failed to load cite db", e);
        try { window.marksmenSources = JSON.parse(localStorage.getItem(SOURCES_KEY) || '[]'); } catch { window.marksmenSources = []; }
    }
}

// Load DB immediately
loadSourcesFromCite();

function saveSources() {
    // Only local legacy saving (if they delete a legacy one)
    localStorage.setItem(SOURCES_KEY, JSON.stringify(window.marksmenSources.filter(s => !s.url?.includes('doi.org'))));
}

/** APA in-text citation: (LastName, Year) */
function apaInText(src) {
    const lastName = (src.author || 'Unknown').split(',')[0].trim().split(' ').pop();
    return `(${lastName}, ${src.year || 'n.d.'})`;
}

/** APA reference list entry (HTML) */
function apaRef(src) {
    const author = src.author || 'Unknown';
    const year   = src.year   || 'n.d.';
    const title  = src.title  || 'Untitled';
    const pub    = src.publisher || '';
    const url    = src.url ? ` Retrieved from ${src.url}` : '';
    switch (src.type) {
        case 'article':
            return `${author} (${year}). ${title}. <em>${pub}</em>.${url}`;
        case 'website':
            return `${author} (${year}). <em>${title}</em>.${url}`;
        default:
            return `${author} (${year}). <em>${title}</em>. ${pub}.${url}`;
    }
}

function renderSourceManagerList() {
    const list = document.getElementById('sm-source-list');
    if (!list) return;
    list.innerHTML = '<div style="font-size:11px;color:var(--text-hint);padding:4px;">Marksmen-Cite database is the authoritative source. Please manage references there.</div>';
}

function renderCitationPicker(filter) {
    const list = document.getElementById('citation-source-list');
    if (!list) return;
    list.innerHTML = '';
    const lf = (filter || '').toLowerCase();
    const filtered = window.marksmenSources.filter(s =>
        !lf || (s.author + s.title + s.year).toLowerCase().includes(lf)
    );
    if (filtered.length === 0) {
        list.innerHTML = '<div style="font-size:11px;color:var(--text-hint);padding:4px;">No sources. Add one via Manage Sources.</div>';
        return;
    }
    filtered.forEach(src => {
        const btn = document.createElement('button');
        btn.className = 'fmenu-item';
        btn.textContent = `${src.author || '?'}, ${src.year || 'n.d.'} — ${src.title || 'Untitled'}`;
        btn.addEventListener('click', () => {
            const editorEl = document.getElementById('editor');
            editorEl.focus();
            const sel = window.getSelection();
            if (sel.rangeCount) {
                const range = sel.getRangeAt(0);
                const cite = document.createElement('cite');
                cite.className   = 'ref-citation';
                cite.dataset.srcId = src.id;
                cite.textContent = apaInText(src);
                range.insertNode(cite);
                range.setStartAfter(cite);
                range.collapse(true);
                sel.removeAllRanges();
                sel.addRange(range);
            }
            document.getElementById('citation-picker').hidden = true;
            editorEl.dispatchEvent(new Event('input'));
            if (window.pushUndoSnapshot) window.pushUndoSnapshot();
        });
        list.appendChild(btn);
    });
}

// ── Wire References ribbon controls ──────────────────────────────────────────
document.addEventListener('DOMContentLoaded', () => {
    // (already past DOMContentLoaded since this loads as a module — call directly)
});

// Citation button
const btnRefCite    = document.getElementById('btn-ref-cite');
const citationPicker = document.getElementById('citation-picker');
const citationSearch = document.getElementById('citation-search');

btnRefCite?.addEventListener('click', (e) => {
    e.stopPropagation();
    citationPicker.hidden = !citationPicker.hidden;
    if (!citationPicker.hidden) {
        const rect = btnRefCite.getBoundingClientRect();
        citationPicker.style.top  = (rect.bottom + 4) + 'px';
        citationPicker.style.left = rect.left + 'px';
        renderCitationPicker('');
        citationSearch?.focus();
    }
});

citationSearch?.addEventListener('input', () => renderCitationPicker(citationSearch.value));

document.getElementById('btn-add-new-source')?.addEventListener('click', () => {
    citationPicker.hidden = true;
    document.getElementById('source-manager-scrim').hidden = false;
    renderSourceManagerList();
});

document.getElementById('btn-manage-sources')?.addEventListener('click', () => {
    document.getElementById('source-manager-scrim').hidden = false;
    renderSourceManagerList();
});

document.getElementById('btn-source-manager-close')?.addEventListener('click', () => {
    document.getElementById('source-manager-scrim').hidden = true;
});

document.getElementById('btn-sm-save')?.addEventListener('click', () => {
    const author    = document.getElementById('sm-author')?.value.trim();
    const year      = document.getElementById('sm-year')?.value.trim();
    const title     = document.getElementById('sm-title')?.value.trim();
    const publisher = document.getElementById('sm-publisher')?.value.trim();
    const type      = document.getElementById('sm-type')?.value;
    const url       = document.getElementById('sm-url')?.value.trim();
    if (!author || !title) {
        if (window.showToast) window.showToast('Author and Title are required.');
        return;
    }
    const src = { id: 'src-' + Date.now().toString(36), author, year, title, publisher, type, url };
    window.marksmenSources.push(src);
    saveSources();
    ['sm-author','sm-year','sm-title','sm-publisher','sm-url'].forEach(id => {
        const el = document.getElementById(id);
        if (el) el.value = '';
    });
    renderSourceManagerList();
    if (window.showToast) window.showToast('Source added.');
});

// ── Bibliography (G-H34) ─────────────────────────────────────────────────────
document.getElementById('btn-ref-bibliography')?.addEventListener('click', () => {
    const editorEl = document.getElementById('editor');
    if (window.marksmenSources.length === 0) {
        if (window.showToast) window.showToast('No sources. Add sources via Manage Sources first.');
        return;
    }
    const sorted = [...window.marksmenSources].sort((a, b) =>
        (a.author || '').localeCompare(b.author || '')
    );
    const section = document.createElement('div');
    section.className = 'ref-bibliography';
    section.contentEditable = 'false';
    section.innerHTML = `
        <h3 style="font-size:14px; font-weight:bold; margin-bottom:8px; border-bottom:1px solid var(--border); padding-bottom:4px;">References</h3>
        ${sorted.map(src => `<p style="margin:4px 0; padding-left:2em; text-indent:-2em; font-size:12px;">${apaRef(src)}</p>`).join('')}
    `;
    editorEl.appendChild(section);
    section.scrollIntoView({ behavior: 'smooth', block: 'start' });
    editorEl.dispatchEvent(new Event('input'));
    if (window.pushUndoSnapshot) window.pushUndoSnapshot();
    if (window.showToast) window.showToast('Bibliography inserted.');
});

// ── Table of Figures / Tables (G-H38) ────────────────────────────────────────
function insertAutoTable(labelPrefix, heading) {
    const editorEl = document.getElementById('editor');
    const figures  = [...editorEl.querySelectorAll('figcaption, caption')].filter(el =>
        el.textContent.trim().toLowerCase().startsWith(labelPrefix.toLowerCase())
    );
    if (figures.length === 0) {
        if (window.showToast) window.showToast(`No ${labelPrefix} captions found. Insert captions first.`);
        return;
    }
    const block = document.createElement('div');
    block.className = 'ref-tof';
    block.contentEditable = 'false';
    block.innerHTML = `
        <h3 style="font-size:13px; font-weight:bold; margin-bottom:6px; border-bottom:1px solid var(--border); padding-bottom:3px;">${heading}</h3>
        ${figures.map(el => `<div style="font-size:12px; display:flex; justify-content:space-between; padding:2px 0; border-bottom:1px dotted var(--border-subtle);"><span>${el.textContent.trim()}</span><span style="opacity:0.5;">……</span></div>`).join('')}
    `;
    editorEl.insertBefore(block, editorEl.firstChild);
    block.scrollIntoView({ behavior: 'smooth', block: 'start' });
    editorEl.dispatchEvent(new Event('input'));
    if (window.pushUndoSnapshot) window.pushUndoSnapshot();
    if (window.showToast) window.showToast(`${heading} inserted.`);
}

document.getElementById('btn-ref-tof')?.addEventListener('click', () => insertAutoTable('Figure', 'List of Figures'));
document.getElementById('btn-ref-tot')?.addEventListener('click', () => insertAutoTable('Table', 'List of Tables'));
document.getElementById('btn-ref-toc')?.addEventListener('click', () =>
    document.getElementById('btn-insert-toc')?.click()
);

// ── Auto-numbered Captions (G-H35) ───────────────────────────────────────────
document.getElementById('btn-ref-caption')?.addEventListener('click', () => {
    const editorEl = document.getElementById('editor');
    const sel  = window.getSelection();
    if (!sel.rangeCount) return;
    const node = sel.anchorNode;
    let n = node;
    while (n && n !== editorEl) n = n.parentNode;
    if (!n) return;

    const figure  = sel.anchorNode?.parentElement?.closest('figure');
    const figType = figure?.querySelector('img') ? 'Figure' : 'Table';
    const figCount = editorEl.querySelectorAll(`figcaption[data-label="${figType}"]`).length + 1;
    const labelText  = `${figType} ${figCount}: `;
    const captionText = prompt('Enter caption text:', '');
    if (!captionText) return;

    if (figure) {
        let cap = figure.querySelector('figcaption');
        if (!cap) { cap = document.createElement('figcaption'); figure.appendChild(cap); }
        cap.dataset.label = figType;
        cap.textContent   = labelText + captionText;
    } else {
        const range  = sel.getRangeAt(0);
        const cap    = document.createElement('p');
        cap.style.cssText = 'font-size:11px; color:var(--text-secondary); text-align:center; margin:4px 0;';
        const tagEl  = document.createElement('figcaption');
        tagEl.dataset.label = figType;
        tagEl.style.display = 'inline';
        tagEl.textContent   = labelText + captionText;
        cap.appendChild(tagEl);
        range.insertNode(cap);
    }
    editorEl.dispatchEvent(new Event('input'));
    if (window.pushUndoSnapshot) window.pushUndoSnapshot();
});

// ── Cross-Reference (G-H36) ───────────────────────────────────────────────────
const btnRefCrossRef = document.getElementById('btn-ref-cross-ref');
const crossRefPicker = document.getElementById('cross-ref-picker');
const crossRefType   = document.getElementById('cross-ref-type');
const crossRefList   = document.getElementById('cross-ref-list');

function populateCrossRefList() {
    if (!crossRefList) return;
    crossRefList.innerHTML = '';
    const editorEl = document.getElementById('editor');
    const type = crossRefType?.value || 'heading';
    let targets = [];
    if (type === 'heading') {
        targets = [...editorEl.querySelectorAll('h1,h2,h3,h4,h5,h6')].map(el => {
            if (!el.id) el.id = 'ref-' + Math.random().toString(36).substr(2,6);
            return { id: el.id, label: el.textContent.trim() };
        });
    } else if (type === 'figure') {
        targets = [...editorEl.querySelectorAll('figcaption[data-label="Figure"]')].map(el => {
            const fig = el.closest('figure');
            if (fig && !fig.id) fig.id = 'ref-fig-' + Math.random().toString(36).substr(2,4);
            return { id: fig?.id || '', label: el.textContent.trim() };
        });
    } else if (type === 'table') {
        targets = [...editorEl.querySelectorAll('figcaption[data-label="Table"]')].map(el => {
            if (!el.id) el.id = 'ref-tbl-' + Math.random().toString(36).substr(2,4);
            return { id: el.id, label: el.textContent.trim() };
        });
    } else if (type === 'footnote') {
        targets = [...editorEl.querySelectorAll('.footnote-marker')].map((el, i) => {
            if (!el.id) el.id = 'fn-' + i;
            return { id: el.id, label: `Footnote ${i + 1}` };
        });
    } else if (type === 'bookmark') {
        targets = [...editorEl.querySelectorAll('a[name]')].map(el => ({
            id: el.getAttribute('name'),
            label: el.getAttribute('name')
        }));
    }

    if (targets.length === 0) {
        crossRefList.innerHTML = `<div style="font-size:11px;color:var(--text-hint);padding:4px;">No ${type}s found in document.</div>`;
        return;
    }
    targets.forEach(t => {
        const btn = document.createElement('button');
        btn.className = 'fmenu-item';
        btn.textContent = t.label;
        btn.dataset.targetId = t.id;
        btn.addEventListener('click', () => {
            crossRefList.querySelectorAll('.fmenu-item').forEach(b => b.classList.remove('selected'));
            btn.classList.add('selected');
        });
        crossRefList.appendChild(btn);
    });
}

btnRefCrossRef?.addEventListener('click', (e) => {
    e.stopPropagation();
    crossRefPicker.hidden = !crossRefPicker.hidden;
    if (!crossRefPicker.hidden) {
        const rect = btnRefCrossRef.getBoundingClientRect();
        crossRefPicker.style.top  = (rect.bottom + 4) + 'px';
        crossRefPicker.style.left = rect.left + 'px';
        populateCrossRefList();
    }
});
crossRefType?.addEventListener('change', populateCrossRefList);

document.getElementById('btn-insert-cross-ref')?.addEventListener('click', () => {
    const editorEl = document.getElementById('editor');
    const activeSel = crossRefList?.querySelector('.fmenu-item.selected');
    if (!activeSel) { if (window.showToast) window.showToast('Select a target first.'); return; }
    const targetId = activeSel.dataset.targetId;
    const label    = activeSel.textContent;
    const anchor   = document.createElement('a');
    anchor.href      = '#' + targetId;
    anchor.className = 'ref-cross-ref';
    anchor.textContent = label;
    const sel = window.getSelection();
    if (sel.rangeCount) {
        const range = sel.getRangeAt(0);
        range.insertNode(anchor);
        range.setStartAfter(anchor);
        range.collapse(true);
        sel.removeAllRanges();
        sel.addRange(range);
    }
    crossRefPicker.hidden = true;
    editorEl.dispatchEvent(new Event('input'));
    if (window.pushUndoSnapshot) window.pushUndoSnapshot();
});

// ── Index (G-H37) ─────────────────────────────────────────────────────────────
document.getElementById('btn-ref-mark-index')?.addEventListener('click', () => {
    const editorEl = document.getElementById('editor');
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed) {
        if (window.showToast) window.showToast('Select text to mark as an index entry.');
        return;
    }
    const range = sel.getRangeAt(0);
    const text  = range.toString().trim();
    if (!text) return;
    const mark = document.createElement('span');
    mark.className = 'index-entry';
    mark.dataset.indexTerm = text;
    mark.style.cssText = 'border-bottom:1px dashed var(--accent);color:var(--accent);font-size:0.85em;';
    try {
        range.surroundContents(mark);
        editorEl.dispatchEvent(new Event('input'));
        if (window.pushUndoSnapshot) window.pushUndoSnapshot();
        if (window.showToast) window.showToast(`Index entry marked: "${text}"`);
    } catch {
        if (window.showToast) window.showToast('Cannot mark a selection spanning multiple block elements.');
    }
});

document.getElementById('btn-ref-gen-index')?.addEventListener('click', () => {
    const editorEl = document.getElementById('editor');
    const entries  = [...editorEl.querySelectorAll('.index-entry')];
    if (entries.length === 0) {
        if (window.showToast) window.showToast('No index entries marked. Use "Mark Entry" first.');
        return;
    }
    const groups = {};
    entries.forEach(el => {
        const term = el.dataset.indexTerm || el.textContent;
        if (!groups[term]) groups[term] = 0;
        groups[term]++;
    });
    const block = document.createElement('div');
    block.className = 'ref-index';
    block.contentEditable = 'false';
    const sortedTerms = Object.keys(groups).sort();
    block.innerHTML = `
        <h3 style="font-size:14px;font-weight:bold;margin-bottom:8px;border-bottom:1px solid var(--border);padding-bottom:4px;">Index</h3>
        ${sortedTerms.map(term => `<div style="font-size:12px;margin:2px 0;"><strong>${term}</strong> <span style="color:var(--text-hint);">${groups[term]} occurrence(s)</span></div>`).join('')}
    `;
    editorEl.appendChild(block);
    block.scrollIntoView({ behavior: 'smooth', block: 'start' });
    editorEl.dispatchEvent(new Event('input'));
    if (window.pushUndoSnapshot) window.pushUndoSnapshot();
    if (window.showToast) window.showToast('Index generated.');
});

// ── Bookmarks (G-M11) ─────────────────────────────────────────────────────────
document.getElementById('btn-ref-bookmark')?.addEventListener('click', () => {
    const editorEl = document.getElementById('editor');
    const name = prompt('Enter bookmark name (letters, numbers, hyphens only):');
    if (!name) return;
    const safeName = name.replace(/[^a-zA-Z0-9\-_]/g, '-').toLowerCase();
    if (!safeName) { if (window.showToast) window.showToast('Invalid bookmark name.'); return; }
    const anchor = document.createElement('a');
    anchor.name = safeName;
    anchor.id   = safeName;
    anchor.className = 'ref-bookmark';
    anchor.style.cssText = 'display:inline-block;width:0;height:0;overflow:hidden;';
    anchor.contentEditable = 'false';
    anchor.title = `Bookmark: ${safeName}`;
    const sel = window.getSelection();
    if (sel.rangeCount) {
        const range = sel.getRangeAt(0);
        range.insertNode(anchor);
        range.setStartAfter(anchor);
        range.collapse(true);
        sel.removeAllRanges();
        sel.addRange(range);
    }
    editorEl.dispatchEvent(new Event('input'));
    if (window.pushUndoSnapshot) window.pushUndoSnapshot();
    if (window.showToast) window.showToast(`Bookmark "${safeName}" inserted.`);
});

// ── Endnotes (G-M20) ─────────────────────────────────────────────────────────
let endnoteCount = 0;
document.getElementById('btn-ref-endnote')?.addEventListener('click', () => {
    const editorEl = document.getElementById('editor');
    const sel = window.getSelection();
    if (!sel.rangeCount) return;
    endnoteCount++;
    const marker = document.createElement('sup');
    marker.className = 'endnote-marker';
    marker.dataset.endnote = endnoteCount;
    marker.style.cssText = 'color:var(--accent);cursor:pointer;font-size:0.75em;';
    marker.textContent = `[${endnoteCount}]`;
    const range = sel.getRangeAt(0);
    range.insertNode(marker);
    range.setStartAfter(marker);
    range.collapse(true);
    sel.removeAllRanges();
    sel.addRange(range);
    // Append endnote entry
    let endnotesSection = editorEl.querySelector('.endnotes-section');
    if (!endnotesSection) {
        endnotesSection = document.createElement('div');
        endnotesSection.className = 'endnotes-section';
        endnotesSection.contentEditable = 'false';
        endnotesSection.innerHTML = '<h3 style="font-size:13px;font-weight:bold;border-top:1px solid var(--border);padding-top:8px;margin-top:16px;">Endnotes</h3>';
        editorEl.appendChild(endnotesSection);
    }
    const noteEl = document.createElement('p');
    noteEl.style.cssText = 'font-size:11px;margin:2px 0;';
    const noteBody = document.createElement('span');
    noteBody.contentEditable = 'true';
    noteBody.textContent = `[${endnoteCount}] `;
    noteEl.appendChild(noteBody);
    endnotesSection.appendChild(noteEl);
    noteBody.focus();
    editorEl.dispatchEvent(new Event('input'));
    if (window.pushUndoSnapshot) window.pushUndoSnapshot();
});

// Footnote alias
document.getElementById('btn-ref-footnote')?.addEventListener('click', () =>
    document.getElementById('btn-insert-footnote')?.click()
);

// ── Dismiss pickers on outside click ─────────────────────────────────────────
document.addEventListener('mousedown', (e) => {
    if (citationPicker && !citationPicker.hidden &&
        !e.target.closest('#citation-picker') && !e.target.closest('#btn-ref-cite')) {
        citationPicker.hidden = true;
    }
    if (crossRefPicker && !crossRefPicker.hidden &&
        !e.target.closest('#cross-ref-picker') && !e.target.closest('#btn-ref-cross-ref')) {
        crossRefPicker.hidden = true;
    }
});

// ── Smart Quotes & Autocorrect (G-M02) ───────────────────────────────────────
document.getElementById('editor')?.addEventListener('keyup', (e) => {
    // Don't transform inside tracked-change nodes
    if (window.isTrackChanges) return;
    if (!['\"', '\'', ' ', 'Enter', '-', ')'].includes(e.key)) return;

    const sel = window.getSelection();
    if (!sel.rangeCount) return;
    const range = sel.getRangeAt(0);
    const node  = range.startContainer;
    if (node.nodeType !== Node.TEXT_NODE) return;
    let text = node.textContent;
    const before = text;

    // em-dash: " -- " → " — "
    text = text.replace(/ -- /g, ' \u2014 ');
    // en-dash between numbers: "2 - 3" → "2–3"
    text = text.replace(/(\d) - (\d)/g, '$1\u2013$2');
    // Ellipsis: "..." → "…"
    text = text.replace(/\.\.\./g, '\u2026');
    // Curly double quotes
    text = text.replace(/(^|\s)"(\S)/g, '$1\u201c$2');
    text = text.replace(/(\S)"(\s|[.,;!?]|$)/g, '$1\u201d$2');
    // Curly single quotes / apostrophes
    text = text.replace(/(^|\s)'(\S)/g, '$1\u2018$2');
    text = text.replace(/(\w)'(\w)/g, '$1\u2019$2');
    text = text.replace(/(\S)'(\s|[.,;!?]|$)/g, '$1\u2019$2');
    // Symbols
    text = text.replace(/\(c\)/gi, '\u00a9');
    text = text.replace(/\(r\)/gi,  '\u00ae');
    text = text.replace(/\(tm\)/gi, '\u2122');

    if (text !== before) {
        const offset = range.startOffset;
        node.textContent = text;
        try {
            range.setStart(node, Math.min(offset, text.length));
            range.collapse(true);
            sel.removeAllRanges();
            sel.addRange(range);
        } catch {}
    }
});

// Expose isTrackChanges for smart quotes check (set by main.js)
// main.js sets window.isTrackChanges = true when Track Changes mode is active.
// We read it here via the global — no direct coupling needed.
