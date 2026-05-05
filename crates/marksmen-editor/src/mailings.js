import { invoke } from './wasm_bridge.js';

// State
let mailDataSource = null; // raw CSV string
let mailHeaders = [];
let mailRecords = []; // array of objects
let currentPreviewIndex = -1;
let isPreviewActive = false;

// DOM Elements
const btnData = document.getElementById('btn-mail-data');
const btnInsert = document.getElementById('btn-mail-insert');
const pickerField = document.getElementById('mail-field-picker');
const btnPreview = document.getElementById('btn-mail-preview');
const btnPrev = document.getElementById('btn-mail-prev');
const btnNext = document.getElementById('btn-mail-next');
const recordCount = document.getElementById('mail-record-count');
const btnFinish = document.getElementById('btn-mail-finish');
const pickerFinish = document.getElementById('mail-finish-picker');
const editor = document.getElementById('editor');

// ── Load CSV Data ────────────────────────────────────────────────────────────
btnData.addEventListener('click', async () => {
    try {
        // Use native dialog to load CSV
        const file_path = await window.__TAURI__.core.invoke('import_file');
        // Actually import_file is designed for standard docs. We need a way to load CSV.
        // Let's use the file picker via standard HTML input since tauri allows input[type=file]
        const input = document.createElement('input');
        input.type = 'file';
        input.accept = '.csv,.json';
        input.onchange = e => {
            const file = e.target.files[0];
            if (!file) return;
            const reader = new FileReader();
            reader.onload = async event => {
                mailDataSource = event.target.result;
                parseSimpleCSV(mailDataSource);
                updateUI();
            };
            reader.readAsText(file);
        };
        input.click();
    } catch(e) {
        console.error(e);
    }
});

// A very naive CSV parser for preview purposes (the backend uses the rigorous `csv` crate)
function parseSimpleCSV(text) {
    const lines = text.split('\n').map(l => l.trim()).filter(l => l.length > 0);
    if (lines.length === 0) return;
    
    // Naive split by comma, ignoring quotes (only for frontend preview/headers)
    mailHeaders = lines[0].split(',').map(h => h.trim());
    mailRecords = [];
    
    for (let i = 1; i < lines.length; i++) {
        const vals = lines[i].split(',').map(v => v.trim());
        const record = {};
        for (let j = 0; j < mailHeaders.length; j++) {
            record[mailHeaders[j]] = vals[j] || '';
        }
        mailRecords.push(record);
    }
}

// ── Update UI ───────────────────────────────────────────────────────────────
function updateUI() {
    // Populate Insert Field dropdown
    if (mailHeaders.length > 0) {
        pickerField.innerHTML = '';
        mailHeaders.forEach(header => {
            const btn = document.createElement('button');
            btn.className = 'fmenu-item';
            btn.textContent = header;
            btn.addEventListener('click', () => {
                pickerField.hidden = true;
                // Insert {{header}} at cursor
                document.execCommand('insertText', false, `{{${header}}}`);
            });
            pickerField.appendChild(btn);
        });
        
        if (currentPreviewIndex === -1 && mailRecords.length > 0) {
            currentPreviewIndex = 0;
        }
        recordCount.textContent = mailRecords.length > 0 ? `${currentPreviewIndex + 1}` : '0';
    } else {
        pickerField.innerHTML = '<div style="font-size:10px; color:var(--text-hint); padding:2px;">No data source loaded</div>';
    }
}

btnInsert.addEventListener('click', (e) => {
    e.stopPropagation();
    pickerField.hidden = !pickerField.hidden;
});

// ── Preview Logic ───────────────────────────────────────────────────────────
let originalHtml = ''; // store the non-preview template

btnPreview.addEventListener('click', () => {
    if (mailRecords.length === 0) return;
    
    isPreviewActive = !isPreviewActive;
    if (isPreviewActive) {
        btnPreview.classList.add('rbtn--active-toggle');
        originalHtml = editor.innerHTML;
        renderPreview();
    } else {
        btnPreview.classList.remove('rbtn--active-toggle');
        editor.innerHTML = originalHtml;
    }
});

function renderPreview() {
    if (!isPreviewActive || mailRecords.length === 0) return;
    
    const record = mailRecords[currentPreviewIndex];
    let html = originalHtml;
    
    // Naive regex replacement for preview
    for (const [key, val] of Object.entries(record)) {
        const regex = new RegExp(`\\{\\{${key}\\}\\}`, 'g');
        html = html.replace(regex, `<span style="background: rgba(255, 212, 59, 0.3); border-radius: 2px;">${val}</span>`);
    }
    
    editor.innerHTML = html;
    recordCount.textContent = `${currentPreviewIndex + 1}`;
}

btnPrev.addEventListener('click', () => {
    if (mailRecords.length === 0) return;
    currentPreviewIndex = (currentPreviewIndex - 1 + mailRecords.length) % mailRecords.length;
    renderPreview();
});

btnNext.addEventListener('click', () => {
    if (mailRecords.length === 0) return;
    currentPreviewIndex = (currentPreviewIndex + 1) % mailRecords.length;
    renderPreview();
});

// ── Finish & Merge ─────────────────────────────────────────────────────────
btnFinish.addEventListener('click', e => {
    e.stopPropagation();
    if (mailRecords.length === 0) {
        alert("Please select a data source first.");
        return;
    }
    pickerFinish.hidden = !pickerFinish.hidden;
});

pickerFinish.addEventListener('click', async e => {
    const btn = e.target.closest('button[data-export]');
    if (!btn) return;
    pickerFinish.hidden = true;
    
    const format = btn.dataset.export;
    
    // We need the markdown of the template.
    // If preview is active, we must use the originalHtml.
    let templateHtml = isPreviewActive ? originalHtml : editor.innerHTML;
    
    try {
        // We simulate `currentMarkdown` retrieval via html_to_md
        const templateMarkdown = await invoke('html_to_md', { html: templateHtml });
        
        // Let the status bar know
        const syncStatus = document.getElementById('sync-status');
        if (syncStatus) {
            syncStatus.textContent = "Merging...";
            syncStatus.className = 'sync-status syncing';
        }
        
        const docName = document.getElementById('doc-name').textContent || 'MailMerge';
        
        await invoke('execute_mail_merge', {
            template_markdown: templateMarkdown,
            csv_data: mailDataSource,
            format: format,
            doc_name: docName + "_Merged"
        });
        
        if (syncStatus) {
            syncStatus.textContent = "● Saved";
            syncStatus.className = 'sync-status';
        }
        
    } catch(err) {
        console.error("Merge failed", err);
        const syncStatus = document.getElementById('sync-status');
        if (syncStatus) {
            syncStatus.textContent = "Merge Error";
            syncStatus.className = 'sync-status error';
        }
        if (err !== "No file selected") {
            alert("Merge failed: " + err);
        }
    }
});

// Hide popups on body click
document.body.addEventListener('click', e => {
    if (!btnInsert.contains(e.target) && !pickerField.contains(e.target)) {
        pickerField.hidden = true;
    }
    if (!btnFinish.contains(e.target) && !pickerFinish.contains(e.target)) {
        pickerFinish.hidden = true;
    }
});
