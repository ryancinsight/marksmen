// marksmen-cite main.js — Phase 20
import { invoke } from './wasm_bridge.js';
// We don't import window.__TAURI__.event listen since event listeners need conditional logic if in browser.
// The browser bridge doesn't support 'listen' right now, but cite doesn't heavily rely on it.
// If it does, we can add a dummy listen to bridge.
const listen = window.__TAURI__ ? window.__TAURI__.event.listen : () => Promise.resolve(() => {});
// ── State ──────────────────────────────────────────────────
let references   = [];
let collections  = [];
let selectedId   = null;
let selectedIds  = new Set();
let saveTimeout  = null;
let currentFilter = 'all';
let sortField    = 'date_added';
let sortDir      = 'desc';
let citationStyle = 'apa';

// ── DOM refs ───────────────────────────────────────────────
const listEl        = document.getElementById('reference-list');
const detailPane    = document.getElementById('detail-pane');
const searchInput   = document.getElementById('search-input');
const syncEl        = document.getElementById('sync-indicator');
const selectAllCb   = document.getElementById('select-all');
const collectionsNav = document.getElementById('collections-nav');

const fType       = document.getElementById('detail-ref-type');
const fTitle      = document.getElementById('detail-title');
const fAuthors    = document.getElementById('detail-authors');
const fAbstract   = document.getElementById('detail-abstract');
const fYear       = document.getElementById('detail-year');
const fJournal    = document.getElementById('detail-journal');
const fVolume     = document.getElementById('detail-volume');
const fIssue      = document.getElementById('detail-issue');
const fPages      = document.getElementById('detail-pages');
const fPublisher  = document.getElementById('detail-publisher');
const fEdition    = document.getElementById('detail-edition');
const fDoi        = document.getElementById('detail-doi');
const fPmid       = document.getElementById('detail-pmid');
const fIsbn       = document.getElementById('detail-isbn');
const fIssn       = document.getElementById('detail-issn');
const fUrl        = document.getElementById('detail-url');
const fAccessDate = document.getElementById('detail-access-date');
const fLanguage   = document.getElementById('detail-language');
const fNotes      = document.getElementById('detail-notes');
const fDateAdded  = document.getElementById('detail-date-added');
const tagsContainer = document.getElementById('tags-container');
const tagsInput   = document.getElementById('tags-input');
const citationOutput = document.getElementById('citation-output');
const citStyleSel = document.getElementById('citation-style');
const btnOpenPdf  = document.getElementById('btn-open-pdf');
const btnStar     = document.getElementById('btn-star');
const btnRead     = document.getElementById('btn-mark-read');

// ── Init ───────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', async () => {
    try {
        [references, collections] = await Promise.all([
            invoke('load_references'),
            invoke('load_collections'),
        ]);
        renderNavCounts();
        renderCollectionsNav();
        renderList();
    } catch (e) { console.error('Init failed:', e); }

    // Listen for Web Importer payloads
    listen('web-import', (event) => {
        const payload = event.payload;
        const now = new Date().toISOString().slice(0,10);
        const ref = {
            id: crypto.randomUUID(),
            reference_type: "Journal Article",
            title: payload.title || "Untitled",
            authors: payload.authors || [],
            abstract_text: payload.abstract_text || "",
            journal: payload.journal || "",
            year: payload.year || "",
            volume: "", issue: "", pages: "",
            publisher: "",
            doi: payload.doi || "",
            pmid: "", isbn: "", issn: "",
            url: payload.url || "",
            language: "", access_date: "",
            tags: [], notes: payload.source ? `Imported from ${payload.source}` : "",
            pdf_path: null,
            starred: false, read_status: false,
            date_added: now,
            date_modified: now,
            collections: []
        };
        references.push(ref);
        renderNavCounts();
        renderList();
        scheduleSave();
        syncEl.textContent = 'Added from Web Importer';
        setTimeout(() => syncEl.textContent = 'All changes saved', 2000);
    });
});

// ── Helpers ───────────────────────────────────────────────
function typeClass(t) {
    const m = { 'Journal Article':'journal','Book':'book','Book Chapter':'book',
        'Conference Paper':'conf','Thesis':'thesis','Preprint':'preprint' };
    return m[t] || 'other';
}
function typeLetter(t) {
    const m = { 'Journal Article':'J','Book':'B','Book Chapter':'B',
        'Conference Paper':'C','Thesis':'T','Report':'R','Preprint':'P','Website':'W','Patent':'P' };
    return m[t] || 'O';
}

function applyFilter(refs) {
    const q = searchInput.value.toLowerCase();
    return refs.filter(r => {
        // text search
        if (q) {
            const hay = [r.title, ...r.authors, r.year, r.journal, r.doi,
                r.pmid, r.isbn, ...r.tags, r.notes].join(' ').toLowerCase();
            if (!hay.includes(q)) return false;
        }
        // sidebar filter
        if (currentFilter === 'starred') return r.starred;
        if (currentFilter === 'unread')  return !r.read_status;
        if (currentFilter === 'recent') {
            const today = new Date();
            const added = new Date(r.date_added);
            return (today - added) < 30 * 86400000; // last 30 days
        }
        if (currentFilter.startsWith('type-')) return r.reference_type === currentFilter.slice(5);
        if (currentFilter.startsWith('col-')) {
            const colId = currentFilter.slice(4);
            const col = collections.find(c => c.id === colId);
            return col ? col.ref_ids.includes(r.id) : false;
        }
        return true;
    });
}

function applySortAndFilter() {
    const filtered = applyFilter(references);
    filtered.sort((a, b) => {
        let av = a[sortField] ?? '';
        let bv = b[sortField] ?? '';
        if (sortField === 'authors') { av = av[0] ?? ''; bv = bv[0] ?? ''; }
        const cmp = String(av).localeCompare(String(bv));
        return sortDir === 'asc' ? cmp : -cmp;
    });
    return filtered;
}

function renderNavCounts() {
    document.getElementById('count-all').textContent    = references.length;
    document.getElementById('count-starred').textContent = references.filter(r => r.starred).length;
    document.getElementById('count-unread').textContent  = references.filter(r => !r.read_status).length;
}

function renderCollectionsNav() {
    collectionsNav.innerHTML = '';
    collections.forEach(col => {
        const li = document.createElement('li');
        li.className = 'nav-item' + (currentFilter === 'col-' + col.id ? ' active' : '');
        li.innerHTML = `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
            <span style="flex:1;overflow:hidden;text-overflow:ellipsis;">${col.name}</span>
            <span class="nav-count">${col.ref_ids.length}</span>`;
        li.addEventListener('click', () => setFilter('col-' + col.id));
        li.addEventListener('contextmenu', e => { e.preventDefault(); openCollectionModal(col); });
        collectionsNav.appendChild(li);
    });
}

// ── Render List ────────────────────────────────────────────
function renderList() {
    const items = applySortAndFilter();
    if (items.length === 0) {
        listEl.innerHTML = `<div class="empty-list-state"><div class="empty-list-icon">📚</div><p>${
            references.length === 0 ? 'Your library is empty.' : 'No references match your filter.'
        }</p></div>`;
        return;
    }
    listEl.innerHTML = '';
    items.forEach(ref => {
        const row = document.createElement('div');
        row.className = 'ref-item' + (ref.id === selectedId ? ' selected' : '');
        row.dataset.id = ref.id;
        const badge = `<span class="type-badge ${typeClass(ref.reference_type)}">${typeLetter(ref.reference_type)}</span>`;
        const authorStr = ref.authors.slice(0, 2).join(', ') + (ref.authors.length > 2 ? ' et al.' : '');
        row.innerHTML = `
            <div class="col col-check"><input type="checkbox" data-id="${ref.id}" ${selectedIds.has(ref.id)?'checked':''}></div>
            <div class="col col-type">${badge}</div>
            <div class="col col-title">${ref.title || 'Untitled'}${ref.starred ? ' ★' : ''}</div>
            <div class="col col-authors">${authorStr || '—'}</div>
            <div class="col col-year">${ref.year || '—'}</div>
            <div class="col col-journal">${ref.journal || '—'}</div>`;
        row.querySelector('input[type="checkbox"]').addEventListener('change', e => {
            e.stopPropagation();
            if (e.target.checked) selectedIds.add(ref.id); else selectedIds.delete(ref.id);
            updateSelectAll();
        });
        row.addEventListener('click', () => selectReference(ref.id));
        listEl.appendChild(row);
    });
    updateSelectAll();
    renderNavCounts();
}

function updateSelectAll() {
    const items = applySortAndFilter();
    selectAllCb.checked = items.length > 0 && items.every(r => selectedIds.has(r.id));
    selectAllCb.indeterminate = !selectAllCb.checked && selectedIds.size > 0;
}

// ── Select Reference ───────────────────────────────────────
function selectReference(id) {
    selectedId = id;
    renderList();
    const ref = references.find(r => r.id === id);
    if (!ref) { detailPane.classList.add('empty'); return; }
    detailPane.classList.remove('empty');

    fType.value          = ref.reference_type || 'Journal Article';
    fTitle.textContent   = ref.title;
    fAuthors.textContent = ref.authors.join('; ');
    fAbstract.textContent= ref.abstract_text;
    fYear.value          = ref.year;
    fJournal.value       = ref.journal;
    fVolume.value        = ref.volume ?? '';
    fIssue.value         = ref.issue  ?? '';
    fPages.value         = ref.pages  ?? '';
    fPublisher.value     = ref.publisher ?? '';
    fEdition.value       = ref.edition   ?? '';
    fDoi.value           = ref.doi;
    fPmid.value          = ref.pmid;
    fIsbn.value          = ref.isbn      ?? '';
    fIssn.value          = ref.issn      ?? '';
    fUrl.value           = ref.url       ?? '';
    fAccessDate.value    = ref.access_date ?? '';
    fLanguage.value      = ref.language  ?? '';
    fNotes.textContent   = ref.notes     ?? '';
    fDateAdded.textContent = ref.date_added ? `Added: ${ref.date_added}` : '';

    btnOpenPdf.style.display = ref.pdf_path ? 'flex' : 'none';
    btnStar.textContent = ref.starred ? '★' : '☆';
    btnStar.classList.toggle('starred', !!ref.starred);
    btnRead.classList.toggle('read-status', !!ref.read_status);

    renderTags(ref.tags ?? []);
    refreshCitation(ref);
}

// ── Tags ───────────────────────────────────────────────────
function renderTags(tags) {
    // Remove existing chips
    tagsContainer.querySelectorAll('.tag-chip').forEach(el => el.remove());
    tags.forEach(t => addTagChip(t));
}
function addTagChip(tag) {
    const chip = document.createElement('span');
    chip.className = 'tag-chip';
    chip.innerHTML = `${tag} <button class="tag-remove">×</button>`;
    chip.querySelector('.tag-remove').addEventListener('click', () => {
        chip.remove();
        updateCurrentRef();
    });
    tagsContainer.insertBefore(chip, tagsInput);
}
tagsInput.addEventListener('keydown', e => {
    if ((e.key === 'Enter' || e.key === ',') && tagsInput.value.trim()) {
        e.preventDefault();
        addTagChip(tagsInput.value.trim());
        tagsInput.value = '';
        updateCurrentRef();
    }
});

// ── Citation Refresh ───────────────────────────────────────
async function refreshCitation(ref) {
    if (!ref) return;
    try {
        const citation = await invoke('format_citation', { reference: ref, style: citationStyle });
        citationOutput.textContent = citation;
    } catch { citationOutput.textContent = '(citation unavailable)'; }
}
citStyleSel.addEventListener('change', () => {
    citationStyle = citStyleSel.value;
    const ref = references.find(r => r.id === selectedId);
    if (ref) refreshCitation(ref);
});

// ── Save ───────────────────────────────────────────────────
function scheduleSave() {
    syncEl.textContent = 'Saving…';
    clearTimeout(saveTimeout);
    saveTimeout = setTimeout(async () => {
        try {
            await invoke('save_references', { references });
            syncEl.textContent = 'All changes saved';
            renderList();
        } catch (e) { syncEl.textContent = 'Save error'; console.error(e); }
    }, 900);
}

function updateCurrentRef() {
    if (!selectedId) return;
    const ref = references.find(r => r.id === selectedId);
    if (!ref) return;

    ref.reference_type = fType.value;
    ref.title          = fTitle.textContent.trim();
    ref.authors        = fAuthors.textContent.split(';').map(s => s.trim()).filter(Boolean);
    ref.abstract_text  = fAbstract.textContent;
    ref.year           = fYear.value;
    ref.journal        = fJournal.value;
    ref.volume         = fVolume.value;
    ref.issue          = fIssue.value;
    ref.pages          = fPages.value;
    ref.publisher      = fPublisher.value;
    ref.edition        = fEdition.value;
    ref.doi            = fDoi.value;
    ref.pmid           = fPmid.value;
    ref.isbn           = fIsbn.value;
    ref.issn           = fIssn.value;
    ref.url            = fUrl.value;
    ref.access_date    = fAccessDate.value;
    ref.language       = fLanguage.value;
    ref.notes          = fNotes.textContent;
    ref.tags           = [...tagsContainer.querySelectorAll('.tag-chip')]
        .map(c => c.firstChild.textContent.trim()).filter(Boolean);
    ref.date_modified  = new Date().toISOString().slice(0,10);

    scheduleSave();
    refreshCitation(ref);
}

[fTitle, fAuthors, fAbstract, fNotes].forEach(el => el.addEventListener('input', updateCurrentRef));
[fType, fYear, fJournal, fVolume, fIssue, fPages, fPublisher, fEdition,
 fDoi, fPmid, fIsbn, fIssn, fUrl, fAccessDate, fLanguage].forEach(el => el.addEventListener('input', updateCurrentRef));

// ── Sort ───────────────────────────────────────────────────
document.querySelectorAll('.list-header .sortable').forEach(col => {
    col.addEventListener('click', () => {
        const field = col.dataset.sort;
        if (sortField === field) { sortDir = sortDir === 'asc' ? 'desc' : 'asc'; }
        else { sortField = field; sortDir = 'asc'; }
        document.querySelectorAll('.sortable').forEach(c => c.classList.remove('asc','desc'));
        col.classList.add(sortDir);
        renderList();
    });
});

// ── Select All ─────────────────────────────────────────────
selectAllCb.addEventListener('change', () => {
    const items = applySortAndFilter();
    if (selectAllCb.checked) items.forEach(r => selectedIds.add(r.id));
    else selectedIds.clear();
    renderList();
});

// ── Search / Filter ────────────────────────────────────────
searchInput.addEventListener('input', renderList);

function setFilter(f) {
    currentFilter = f;
    document.querySelectorAll('.nav-item').forEach(el => {
        el.classList.toggle('active',
            el.dataset.filter === f || (f.startsWith('col-') && el === document.querySelector(`[data-col="${f.slice(4)}"]`)));
    });
    // update nav items by data-filter
    document.querySelectorAll('[data-filter]').forEach(el =>
        el.classList.toggle('active', el.dataset.filter === f));
    renderList();
}
document.querySelectorAll('[data-filter]').forEach(el =>
    el.addEventListener('click', () => setFilter(el.dataset.filter)));

// ── Star / Read ────────────────────────────────────────────
btnStar.addEventListener('click', () => {
    const ref = references.find(r => r.id === selectedId); if (!ref) return;
    ref.starred = !ref.starred;
    btnStar.textContent = ref.starred ? '★' : '☆';
    btnStar.classList.toggle('starred', ref.starred);
    scheduleSave();
});
btnRead.addEventListener('click', () => {
    const ref = references.find(r => r.id === selectedId); if (!ref) return;
    ref.read_status = !ref.read_status;
    btnRead.classList.toggle('read-status', ref.read_status);
    scheduleSave();
});

// ── Import PDF ─────────────────────────────────────────────
document.getElementById('btn-import-pdf').addEventListener('click', async () => {
    syncEl.textContent = 'Importing PDF…';
    try {
        const ref = await invoke('import_pdf');
        if (ref) { references.push(ref); scheduleSave(); selectReference(ref.id); }
        else { syncEl.textContent = 'All changes saved'; }
    } catch (e) { syncEl.textContent = 'Import failed'; alert(`PDF import failed: ${e}`); }
});

// ── Fetch DOI ──────────────────────────────────────────────
document.getElementById('btn-fetch-doi').addEventListener('click', async () => {
    const doi = prompt('Enter DOI (e.g. 10.1038/nature12373):');
    if (!doi?.trim()) return;
    syncEl.textContent = 'Fetching DOI…';
    try {
        const ref = await invoke('fetch_doi', { doi: doi.trim() });
        references.push(ref); scheduleSave(); selectReference(ref.id);
    } catch (e) { syncEl.textContent = 'Fetch failed'; alert(`DOI fetch failed: ${e}`); }
});

// ── Fetch PubMed ───────────────────────────────────────────
document.getElementById('btn-fetch-pmid').addEventListener('click', async () => {
    const pmid = prompt('Enter PubMed ID (PMID):');
    if (!pmid?.trim()) return;
    syncEl.textContent = 'Fetching PubMed…';
    try {
        const ref = await invoke('fetch_pmid', { pmid: pmid.trim() });
        references.push(ref); scheduleSave(); selectReference(ref.id);
    } catch (e) { syncEl.textContent = 'Fetch failed'; alert(`PubMed fetch failed: ${e}`); }
});

// ── Fetch arXiv ────────────────────────────────────────────
document.getElementById('btn-fetch-arxiv').addEventListener('click', async () => {
    const id = prompt('Enter arXiv ID (e.g. 2301.01234 or full URL):');
    if (!id?.trim()) return;
    syncEl.textContent = 'Fetching arXiv…';
    try {
        const ref = await invoke('fetch_arxiv', { arxivId: id.trim() });
        references.push(ref); scheduleSave(); selectReference(ref.id);
    } catch (e) { syncEl.textContent = 'Fetch failed'; alert(`arXiv fetch failed: ${e}`); }
});

// ── Fetch ISBN ─────────────────────────────────────────────
document.getElementById('btn-fetch-isbn').addEventListener('click', async () => {
    const isbn = prompt('Enter ISBN (10 or 13 digits):');
    if (!isbn?.trim()) return;
    syncEl.textContent = 'Fetching ISBN…';
    try {
        const ref = await invoke('fetch_isbn', { isbn: isbn.trim() });
        references.push(ref); scheduleSave(); selectReference(ref.id);
    } catch (e) { syncEl.textContent = 'Fetch failed'; alert(`ISBN fetch failed: ${e}`); }
});

// ── Import RIS/BibTeX ──────────────────────────────────────
document.getElementById('btn-import-lib').addEventListener('click', async () => {
    syncEl.textContent = 'Importing library…';
    try {
        const imported = await invoke('import_lib_file');
        if (imported?.length) {
            references.push(...imported); scheduleSave(); renderList();
            alert(`Imported ${imported.length} references.`);
            selectReference(imported[0].id);
        } else { syncEl.textContent = 'All changes saved'; }
    } catch (e) { syncEl.textContent = 'Import failed'; alert(`Import failed: ${e}`); }
});

// ── Add Manual ─────────────────────────────────────────────
document.getElementById('btn-add-manual').addEventListener('click', () => {
    const now = new Date().toISOString().slice(0,10);
    const ref = {
        id: crypto.randomUUID(),
        reference_type: 'Journal Article', title: 'New Reference',
        authors: [], abstract_text: '', journal: '', volume: '', issue: '',
        pages: '', publisher: '', edition: '', doi: '', pmid: '', isbn: '',
        issn: '', url: '', access_date: '', language: '', tags: [], notes: '',
        starred: false, read_status: false, pdf_path: null, year: '',
        date_added: now, date_modified: now, collections: [],
    };
    references.push(ref); scheduleSave(); selectReference(ref.id);
});

// ── Delete ──────────────────────────────────────────────────
document.getElementById('btn-delete-ref').addEventListener('click', () => {
    if (!selectedId || !confirm('Delete this reference?')) return;
    references = references.filter(r => r.id !== selectedId);
    selectedId = null; detailPane.classList.add('empty');
    scheduleSave(); renderList();
});

// ── Update from Web ────────────────────────────────────────
document.getElementById('btn-update-web').addEventListener('click', async () => {
    const ref = references.find(r => r.id === selectedId);
    if (!ref) return;
    if (!ref.doi && !ref.pmid) { alert('No DOI or PMID on this reference.'); return; }
    syncEl.textContent = 'Updating from web…';
    try {
        let updated;
        if (ref.doi) updated = await invoke('fetch_doi', { doi: ref.doi.trim() });
        else updated = await invoke('fetch_pmid', { pmid: ref.pmid.trim() });
        updated.id = ref.id; updated.pdf_path = ref.pdf_path;
        updated.notes = ref.notes; updated.tags = ref.tags;
        updated.starred = ref.starred; updated.read_status = ref.read_status;
        updated.date_added = ref.date_added; updated.collections = ref.collections;
        const idx = references.findIndex(r => r.id === ref.id);
        if (idx !== -1) { references[idx] = updated; scheduleSave(); selectReference(ref.id); }
    } catch (e) { syncEl.textContent = 'Update failed'; alert(`Update failed: ${e}`); }
});

// ── Copy Citation ──────────────────────────────────────────
document.getElementById('btn-copy-citation').addEventListener('click', () => {
    const text = citationOutput.textContent;
    if (!text || text === '(citation unavailable)') return;
    navigator.clipboard.writeText(text).then(() => { syncEl.textContent = 'Citation copied!'; setTimeout(() => syncEl.textContent = 'All changes saved', 1500); });
});

// ── Open PDF ───────────────────────────────────────────────────────────────
btnOpenPdf.addEventListener('click', async () => {
    const ref = references.find(r => r.id === selectedId);
    if (!ref || !ref.pdf_path) return;
    syncEl.textContent = 'Opening PDF...';
    try {
        await invoke('open_pdf_native', { path: ref.pdf_path });
        setTimeout(() => syncEl.textContent = 'All changes saved', 1500);
    } catch (e) {
        syncEl.textContent = 'Open failed';
        alert(`Failed to open PDF: ${e}`);
    }
});

// ── Copy Markdown ──────────────────────────────────────────
document.getElementById('btn-copy-md').addEventListener('click', () => {
    const ref = references.find(r => r.id === selectedId);
    if (!ref) return;
    const md = generateMarkdown(ref);
    navigator.clipboard.writeText(md);
    syncEl.textContent = 'Markdown copied!';
    setTimeout(() => syncEl.textContent = 'All changes saved', 1500);
});

function generateMarkdown(ref) {
    return `---\nid: ${ref.id}\ntype: reference\nreference_type: "${ref.reference_type}"\ntitle: "${ref.title.replace(/"/g, '\\"')}"\nauthors: [${ref.authors.map(a=>`"${a}"`).join(', ')}]\nyear: ${ref.year}\njournal: "${ref.journal}"\nvolume: "${ref.volume}"\nissue: "${ref.issue}"\npages: "${ref.pages}"\ndoi: "${ref.doi}"\npmid: "${ref.pmid}"\n---\n\n# ${ref.title}\n\n**Authors:** ${ref.authors.join(', ')}\n**Journal:** ${ref.journal} (${ref.year})\n\n## Abstract\n${ref.abstract_text}\n`;
}

// ── Find Duplicates ────────────────────────────────────────
document.getElementById('btn-find-duplicates').addEventListener('click', () => {
    const seen = new Map(); const dupes = [];
    references.forEach((ref, i) => {
        if (ref.doi) {
            const key = ref.doi.toLowerCase().trim();
            if (seen.has(key)) dupes.push(i); else seen.set(key, i);
        }
    });
    if (dupes.length === 0) { alert('No duplicates found by DOI.'); return; }
    if (confirm(`Found ${dupes.length} duplicate(s). Remove them?`)) {
        dupes.sort((a,b)=>b-a).forEach(i => references.splice(i,1));
        if (selectedId && !references.find(r => r.id === selectedId)) {
            selectedId = null; detailPane.classList.add('empty');
        }
        scheduleSave(); renderList(); alert('Duplicates removed.');
    }
});

// ── Batch Export ───────────────────────────────────────────
const exportModal = document.getElementById('export-modal');
document.getElementById('btn-batch-export').addEventListener('click', () => {
    const count = selectedIds.size || references.length;
    document.getElementById('export-count-label').textContent =
        selectedIds.size ? `${selectedIds.size} references selected` : `All ${references.length} references`;
    exportModal.style.display = 'flex';
});
document.getElementById('btn-close-export-modal').addEventListener('click', () => exportModal.style.display = 'none');

async function doExport(format) {
    const toExport = selectedIds.size ? references.filter(r => selectedIds.has(r.id)) : references;
    try {
        const content = await invoke(format === 'ris' ? 'export_ris' : 'export_bibtex', { references: toExport });
        const blob = new Blob([content], { type: 'text/plain' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url; a.download = `marksmen-export.${format}`;
        a.click(); URL.revokeObjectURL(url);
    } catch (e) { alert(`Export failed: ${e}`); }
    exportModal.style.display = 'none';
}
document.getElementById('btn-export-ris').addEventListener('click', () => doExport('ris'));
document.getElementById('btn-export-bib').addEventListener('click', () => doExport('bib'));

// ── Collections ────────────────────────────────────────────
let editingCollection = null;
const colModal      = document.getElementById('collection-modal');
const colNameInput  = document.getElementById('collection-name-input');
const colModalTitle = document.getElementById('collection-modal-title');

function openCollectionModal(col) {
    editingCollection = col || null;
    colModalTitle.textContent = col ? 'Rename Collection' : 'New Collection';
    colNameInput.value = col ? col.name : '';
    colModal.style.display = 'flex';
    colNameInput.focus();
}
document.getElementById('btn-new-collection').addEventListener('click', () => openCollectionModal(null));
document.getElementById('btn-close-collection-modal').addEventListener('click', () => colModal.style.display = 'none');
document.getElementById('btn-save-collection').addEventListener('click', async () => {
    const name = colNameInput.value.trim();
    if (!name) return;
    if (editingCollection) {
        editingCollection.name = name;
    } else {
        collections.push({ id: crypto.randomUUID(), name, ref_ids: [] });
    }
    colModal.style.display = 'none';
    await invoke('save_collections', { collections });
    renderCollectionsNav();
});

// ── Preview Modal (Markdown) ───────────────────────────────
const previewModal   = document.getElementById('preview-modal');
const previewContent = document.getElementById('preview-content');
document.getElementById('btn-close-modal').addEventListener('click', () => previewModal.style.display = 'none');
