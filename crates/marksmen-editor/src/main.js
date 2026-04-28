// ── Tauri IPC shim (browser debug only) ─────────────────────────────────────
if (!window.__TAURI__) {
    window.__TAURI__ = {
        core: {
            invoke: async (cmd, args) => {
                console.log('[Mock IPC]', cmd, args);
                if (cmd === 'html_to_md') return '# Mock\n\nMocked sync.\n';
                if (cmd === 'md_to_html') return '<h1>Mock</h1><p>Mocked html.</p>';
                if (cmd === 'import_file') return ['# Imported\n\nMock file contents.\n', 'mock.md'];
                if (cmd === 'generate_diff') return '<p><del>Old text</del> <ins>New text</ins></p>';
                if (cmd === 'export_format') return ['bW9jaw==', 'text/plain', 'document.txt'];
                throw new Error('Unknown mock command: ' + cmd);
            }
        }
    };
}
const { invoke } = window.__TAURI__.core;

// ── Window-size lock ─────────────────────────────────────────────────────────
// .app-shell uses position:fixed; inset:0 so it is always anchored directly
// to the WebView2 viewport — no document-height influence possible.
// setEditorContent wraps innerHTML assignment to keep a single call-site.
function setEditorContent(html) {
    editor.innerHTML = html;
}

// ── DOM refs ─────────────────────────────────────────────────────────────────
const editor      = document.getElementById('editor');
const syncStatus  = document.getElementById('sync-status');
const pageCount   = document.getElementById('page-count');
const wordCount   = document.getElementById('word-count');
const commentList = document.getElementById('comments-list');
const outlineList = document.getElementById('outline-list');
const sidebar     = document.getElementById('sidebar');
const fileMenu    = document.getElementById('file-menu');
const fileScrim   = document.getElementById('file-menu-scrim');
const findBar     = document.getElementById('find-bar');

// ── State ────────────────────────────────────────────────────────────────────
let currentMarkdown = '';
let baseMarkdown    = '';
let isDiffMode      = false;
let syncTimer       = null;
let currentDocName  = 'Untitled Document';
window.marksmenComments = {};

// ── Theme toggle ─────────────────────────────────────────────────────────────
document.getElementById('btn-toggle-theme').addEventListener('click', () => {
    document.body.classList.toggle('dark-theme');
});

// ── Ribbon tab switching ─────────────────────────────────────────────────────
document.querySelectorAll('.rtab[data-panel]').forEach(tab => {
    tab.addEventListener('click', () => {
        document.querySelectorAll('.rtab[data-panel]').forEach(t => t.classList.remove('rtab--active'));
        document.querySelectorAll('.ribbon-panel').forEach(p => p.classList.remove('ribbon-panel--active'));
        tab.classList.add('rtab--active');
        document.getElementById(tab.dataset.panel).classList.add('ribbon-panel--active');
    });
});

// ── File menu ────────────────────────────────────────────────────────────────
document.getElementById('tab-file-btn').addEventListener('click', () => {
    const hidden = fileMenu.hidden;
    fileMenu.hidden  = !hidden;
    fileScrim.hidden = !hidden;
});
fileScrim.addEventListener('click', () => {
    fileMenu.hidden  = true;
    fileScrim.hidden = true;
});

// ── New document ─────────────────────────────────────────────────────────────
document.getElementById('btn-new').addEventListener('click', () => {
    setEditorContent('<h1>Untitled Document</h1><p></p>');
    currentMarkdown  = '';
    baseMarkdown     = '';
    currentDocName   = 'Untitled Document';
    document.getElementById('doc-name').textContent = currentDocName;
    fileMenu.hidden  = true;
    fileScrim.hidden = true;
    editor.focus();
    updatePageCount();
});

// ── Open / Import ─────────────────────────────────────────────────────────────
document.getElementById('btn-open').addEventListener('click', async () => {
    fileMenu.hidden  = true;
    fileScrim.hidden = true;
    setStatus('Opening…', 'syncing');
    try {
        const [md, filename] = await invoke('import_file');
        currentMarkdown = md;
        baseMarkdown    = md;
        // Strip the file extension for display and export stem:
        // e.g. "thesis.pdf" → displayName="thesis", currentDocName="thesis"
        const lastDot = filename.lastIndexOf('.');
        const displayName = lastDot > 0 ? filename.slice(0, lastDot) : filename;
        currentDocName  = displayName;
        
        document.getElementById('doc-name').textContent = displayName;
        document.title = displayName + ' – Marksmen';
        updatePageCount();
        
        // Extract comment metadata block if present
        const scriptMatch = md.match(/<script type="application\/vnd\.marksmen\.comments">([\s\S]*?)<\/script>/);
        if (scriptMatch && scriptMatch[1]) {
            try {
                window.marksmenComments = JSON.parse(scriptMatch[1]);
            } catch(e) { console.error("Failed to parse comments", e); }
        } else {
            window.marksmenComments = {};
        }

        const html = await invoke('md_to_html', { markdown: md });
        setEditorContent(html);
        renderComments();
        updateWordCount();
        renderOutline();
        
        // Auto-open sidebar when document contains comments
        const hasComments = Object.keys(window.marksmenComments).length > 0;
        if (hasComments) {
            sidebar.classList.remove('collapsed');
            setTimeout(drawArrows, 200);
        }
        setStatus('● Saved');
        document.dispatchEvent(new CustomEvent('marksmen:opened', { detail: { name: currentDocName } }));

    } catch(e) {
        if (e !== 'No file selected') {
            setStatus('Error opening file', 'error');
            console.error(e);
        } else {
            setStatus('● Saved');
        }
    }
});

// ── Export ────────────────────────────────────────────────────────────────────
document.querySelectorAll('.fmenu-export').forEach(btn => {
    btn.addEventListener('click', async () => {
        fileMenu.hidden  = true;
        fileScrim.hidden = true;
        const format = btn.dataset.export;
        setStatus(`Exporting ${format.toUpperCase()}…`, 'syncing');
        try {
            await flush();
            const [b64, mime, filename] = await invoke('export_format', {
                markdown: currentMarkdown,
                format,
                doc_name: currentDocName.replace(/\.[^.]+$/, '') // strip existing extension
            });
            const a = document.createElement('a');
            a.href     = `data:${mime};base64,${b64}`;
            a.download = filename;
            a.click();
            setStatus('● Saved');
        } catch(e) {
            setStatus('Export error', 'error');
            console.error(e);
        }
    });
});

// ── Formatting commands ───────────────────────────────────────────────────────
document.querySelectorAll('[data-cmd]').forEach(btn => {
    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const { cmd, val } = btn.dataset;
        document.execCommand(cmd, false, val || null);
        editor.dispatchEvent(new Event('input'));
    });
});

// Font family + size pickers
document.getElementById('font-family-picker').addEventListener('change', e => {
    document.execCommand('fontName', false, e.target.value);
    editor.focus();
});
document.getElementById('font-size-picker').addEventListener('change', e => {
    // execCommand fontSize only takes 1-7; use style instead
    const size = e.target.value + 'pt';
    const sel = window.getSelection();
    if (sel.rangeCount) {
        const range = sel.getRangeAt(0);
        const span  = document.createElement('span');
        span.style.fontSize = size;
        try { range.surroundContents(span); } catch(_) {}
    }
    editor.focus();
});

// ── Insert: Link ──────────────────────────────────────────────────────────────
document.getElementById('btn-insert-link').addEventListener('click', () => {
    const url = prompt('Enter URL:', 'https://');
    if (url) document.execCommand('createLink', false, url);
});

// ── Insert: Horizontal Rule ──────────────────────────────────────────────────
document.getElementById('btn-insert-hline').addEventListener('click', () => {
    document.execCommand('insertHorizontalRule');
    editor.dispatchEvent(new Event('input'));
});

// ── Insert: Picture ───────────────────────────────────────────────────────────
document.getElementById('btn-insert-picture').addEventListener('click', () => {
    const fileInput = document.createElement('input');
    fileInput.type   = 'file';
    fileInput.accept = 'image/*';
    fileInput.addEventListener('change', () => {
        const file = fileInput.files[0];
        if (!file) return;
        const reader = new FileReader();
        reader.onload = e => {
            const img = `<img src="${e.target.result}" alt="${file.name}" style="max-width:100%">`;
            document.execCommand('insertHTML', false, img);
            editor.dispatchEvent(new Event('input'));
        };
        reader.readAsDataURL(file);
    });
    fileInput.click();
});

// ── Table Engine ─────────────────────────────────────────────────────────────────────────
// —— Grid Picker
const TPICKER_ROWS = 8;
const TPICKER_COLS = 10;
const tablePicker  = document.getElementById('table-picker');
const tpickerGrid  = document.getElementById('tpicker-grid');
const tpickerLabel = document.getElementById('tpicker-label');

// Build grid cells once
(function buildGrid() {
    for (let r = 1; r <= TPICKER_ROWS; r++) {
        for (let c = 1; c <= TPICKER_COLS; c++) {
            const cell = document.createElement('div');
            cell.className = 'tpicker-cell';
            cell.dataset.row = r;
            cell.dataset.col = c;
            tpickerGrid.appendChild(cell);
        }
    }
})();

function setPickerHighlight(rows, cols) {
    tpickerLabel.textContent = rows > 0 ? `${rows} × ${cols} Table` : 'Insert Table';
    [...tpickerGrid.children].forEach(cell => {
        const r = +cell.dataset.row, c = +cell.dataset.col;
        const active = r <= rows && c <= cols;
        cell.classList.toggle('tpicker-cell--hover', active);
    });
}
function openTablePicker() {
    setPickerHighlight(0, 0);
    const rect = document.getElementById('btn-insert-table').getBoundingClientRect();
    tablePicker.style.left = rect.left + 'px';
    tablePicker.style.top  = (rect.bottom + 4) + 'px';
    tablePicker.hidden = false;
}
function closeTablePicker() {
    tablePicker.hidden = true;
    setPickerHighlight(0, 0);
}
function insertTable(rows, cols) {
    let html = '<table><thead><tr>';
    for (let c = 0; c < cols; c++) html += `<th contenteditable="true">Header ${c + 1}</th>`;
    html += '</tr></thead><tbody>';
    for (let r = 0; r < rows - 1; r++) {
        html += '<tr>';
        for (let c = 0; c < cols; c++) html += `<td contenteditable="true"> </td>`;
        html += '</tr>';
    }
    html += '</tbody></table><p></p>';
    document.execCommand('insertHTML', false, html);
    editor.dispatchEvent(new Event('input'));
}

document.getElementById('btn-insert-table').addEventListener('click', e => {
    e.stopPropagation();
    tablePicker.hidden ? openTablePicker() : closeTablePicker();
});
tpickerGrid.addEventListener('mouseover', e => {
    const cell = e.target.closest('.tpicker-cell');
    if (!cell) return;
    setPickerHighlight(+cell.dataset.row, +cell.dataset.col);
});
tpickerGrid.addEventListener('mouseleave', () => setPickerHighlight(0, 0));
tpickerGrid.addEventListener('click', e => {
    const cell = e.target.closest('.tpicker-cell');
    if (!cell) return;
    insertTable(+cell.dataset.row, +cell.dataset.col);
    closeTablePicker();
});
document.getElementById('tpicker-manual').addEventListener('click', () => {
    closeTablePicker();
    const rows = parseInt(prompt('Number of rows:', '3'), 10) || 3;
    const cols = parseInt(prompt('Number of columns:', '3'), 10) || 3;
    insertTable(rows, cols);
});
document.addEventListener('click', e => {
    if (!tablePicker.hidden &&
        !tablePicker.contains(e.target) &&
        e.target.id !== 'btn-insert-table') {
        closeTablePicker();
    }
});

// ── Insert: Footnote ──────────────────────────────────────────────────────────
document.getElementById('btn-insert-footnote').addEventListener('click', () => {
    const defs = editor.querySelectorAll('.footnote-def');
    const label = (defs.length + 1).toString();
    const content = prompt(`Enter footnote text for [${label}]:`);
    if (!content) return;
    
    // Insert inline reference at cursor
    const refHtml = `<sup class="footnote-ref" data-label="${label}">[${label}]</sup>`;
    document.execCommand('insertHTML', false, refHtml);
    
    // Append definition block to the end of the editor
    const defDiv = document.createElement('div');
    defDiv.className = 'footnote-def';
    defDiv.dataset.label = label;
    defDiv.innerHTML = `<b>[${label}]</b>: ${content}`;
    editor.appendChild(defDiv);
    
    editor.dispatchEvent(new Event('input'));
});

// ── Insert: Equation ──────────────────────────────────────────────────────────
document.getElementById('btn-insert-equation').addEventListener('click', () => {
    const isDisplay = confirm('Insert as display (block) equation? OK for Yes, Cancel for Inline.');
    const math = prompt('Enter LaTeX equation:');
    if (!math) return;
    
    // We insert a placeholder span/div that will roundtrip through md_to_html
    // which triggers latex2mathml on the backend and returns rendered HTML.
    let html = '';
    if (isDisplay) {
        html = `<div class="math-display">$$${math}$$</div><p></p>`;
    } else {
        html = `<span class="math-inline">$${math}$</span>`;
    }
    
    document.execCommand('insertHTML', false, html);
    
    // Immediate flush to trigger the backend math renderer via html_to_md -> md_to_html
    flush().then(() => {
        invoke('md_to_html', { markdown: currentMarkdown }).then(res => {
            setEditorContent(res);
        });
    });
});

// ── Layout toggles ────────────────────────────────────────────────────────────
document.getElementById('btn-layout-print').addEventListener('click', () => {
    document.getElementById('page-canvas').style.alignItems = 'center';
    editor.style.width     = 'var(--page-w)';
    editor.style.maxWidth  = '';
    updatePageCount();
});
document.getElementById('btn-layout-web').addEventListener('click', () => {
    document.getElementById('page-canvas').style.alignItems = 'stretch';
    editor.style.width     = '100%';
    editor.style.maxWidth  = '900px';
    updatePageCount();
});

// ── Insert Comment ────────────────────────────────────────────────────────────
document.getElementById('btn-insert-comment').addEventListener('click', () => {
    const sel = window.getSelection();
    if (!sel.rangeCount || sel.isCollapsed) {
        alert('Select some text first, then click Comment.');
        return;
    }
    // Ensure selection is inside editor
    let node = sel.anchorNode;
    while (node && node !== editor) node = node.parentNode;
    if (!node) return;

    const noteText = prompt('Enter comment:');
    if (!noteText) return;

    const range   = sel.getRangeAt(0);
    const context = range.cloneContents().textContent.slice(0, 60);
    const markId  = 'c-' + Date.now().toString(36) + Math.random().toString(36).substr(2, 5);
    
    const mark    = document.createElement('mark');
    mark.className     = 'comment';
    mark.dataset.id    = markId;
    
    // Store in global state instead of dataset
    window.marksmenComments[markId] = {
        author: settings?.author || 'You',
        date: new Date().toISOString(),
        body: noteText,
        context: context,
        thread: []
    };
    
    try {
        range.surroundContents(mark);
        editor.dispatchEvent(new Event('input'));
        renderComments();
    } catch(_) {
        alert('Cannot wrap a selection that spans multiple block elements.');
    }
});

function renderComments() {
    const marks = [...editor.querySelectorAll('mark.comment')];
    if (marks.length === 0) {
        commentList.innerHTML = '<div class="sidebar-empty">No comments yet.<br>Select text and click <em>💬 Comment</em>.</div>';
        drawArrows();
        return;
    }
    commentList.innerHTML = '';
    
    // Prune state: remove comments that were deleted from DOM
    const currentIds = new Set(marks.map(m => m.dataset.id).filter(Boolean));
    for (const id in window.marksmenComments) {
        if (!currentIds.has(id)) delete window.marksmenComments[id];
    }
    
    marks.forEach(mark => {
        let id = mark.dataset.id;
        if (!id) {
            // Upgrade legacy comments
            id = 'c-' + Date.now().toString(36) + Math.random().toString(36).substr(2, 5);
            mark.dataset.id = id;
            window.marksmenComments[id] = {
                author: mark.dataset.author || 'You',
                date: new Date().toISOString(),
                body: mark.dataset.content || '',
                context: mark.dataset.context || '',
                thread: []
            };
            // Remove legacy attrs
            mark.removeAttribute('data-author');
            mark.removeAttribute('data-content');
            mark.removeAttribute('data-context');
        }
        
        const data = window.marksmenComments[id];
        if (!data) return; // Wait for next sync if corrupted
        
        const card = document.createElement('div');
        card.className = 'comment-card';
        card.id = 'card-' + id;
        
        let threadHtml = '';
        if (data.thread && data.thread.length > 0) {
            threadHtml = '<div style="margin-top:8px; padding-top:8px; border-top:1px solid var(--border-subtle);">';
            data.thread.forEach(reply => {
                threadHtml += `<div style="margin-bottom:6px;">
                    <span style="font-size:10px; font-weight:600; color:var(--accent);">${reply.author}</span>
                    <div style="font-size:12px; margin-top:2px;">${reply.body}</div>
                </div>`;
            });
            threadHtml += '</div>';
        }
        
        card.innerHTML = `
            <div style="display:flex; justify-content:space-between; align-items:center;">
                <div class="comment-author">${data.author}</div>
                <button class="delete-btn" style="background:none; border:none; cursor:pointer; font-size:12px; color:var(--text-hint);" title="Delete comment">✕</button>
            </div>
            <div class="comment-context">"${data.context || '…'}"</div>
            <div class="comment-body">${data.body}</div>
            ${threadHtml}
            <div style="margin-top:8px; display:flex; gap:4px;">
                <input type="text" placeholder="Reply..." style="flex:1; border:1px solid var(--border); border-radius:3px; padding:2px 6px; font-size:11px; background:var(--chrome-bg); color:var(--text-primary);">
                <button class="reply-btn" style="background:var(--accent); color:white; border:none; border-radius:3px; padding:2px 8px; font-size:11px; cursor:pointer;">Reply</button>
            </div>
        `;
        
        // Wire events
        card.addEventListener('mouseenter', () => { mark.classList.add('active'); drawArrows(); });
        card.addEventListener('mouseleave', () => { mark.classList.remove('active'); drawArrows(); });
        mark.addEventListener('mouseenter', () => { card.classList.add('active'); card.scrollIntoView({behavior:'smooth', block:'nearest'}); drawArrows(); });
        mark.addEventListener('mouseleave', () => { card.classList.remove('active'); drawArrows(); });
        
        card.querySelector('.delete-btn').addEventListener('click', () => {
            // Unwrap the mark
            const parent = mark.parentNode;
            while(mark.firstChild) parent.insertBefore(mark.firstChild, mark);
            parent.removeChild(mark);
            delete window.marksmenComments[id];
            editor.dispatchEvent(new Event('input'));
            renderComments();
        });
        
        const replyInput = card.querySelector('input');
        card.querySelector('.reply-btn').addEventListener('click', () => {
            const val = replyInput.value.trim();
            if (val) {
                if (!data.thread) data.thread = [];
                data.thread.push({ author: settings?.author || 'You', date: new Date().toISOString(), body: val });
                replyInput.value = '';
                editor.dispatchEvent(new Event('input'));
                renderComments();
            }
        });
        
        commentList.appendChild(card);
    });
    
    drawArrows();
}

function drawArrows() {
    const svg = document.getElementById('comment-arrows');
    svg.innerHTML = '';
    
    if (sidebar.classList.contains('collapsed')) return;
    
    const marks = [...editor.querySelectorAll('mark.comment')];
    marks.forEach(mark => {
        const id = mark.dataset.id;
        if (!id) return;
        const card = document.getElementById('card-' + id);
        if (!card) return;
        
        const mRect = mark.getBoundingClientRect();
        const cRect = card.getBoundingClientRect();
        
        // Don't draw if card is invisible
        if (cRect.top > window.innerHeight || cRect.bottom < 0) return;
        
        const isHovered = mark.classList.contains('active') || card.classList.contains('active');
        
        const startX = cRect.left;
        const startY = cRect.top + 16;
        const endX = mRect.right;
        const endY = mRect.top + (mRect.height / 2);
        
        // Bezier curve
        const controlX1 = startX - 50;
        const controlY1 = startY;
        const controlX2 = endX + 50;
        const controlY2 = endY;
        
        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.setAttribute('d', `M ${startX} ${startY} C ${controlX1} ${controlY1}, ${controlX2} ${controlY2}, ${endX} ${endY}`);
        path.setAttribute('fill', 'none');
        path.setAttribute('stroke', isHovered ? 'var(--accent)' : 'var(--border)');
        path.setAttribute('stroke-width', isHovered ? '2' : '1.5');
        if (!isHovered) {
            path.setAttribute('stroke-dasharray', '4 4');
            path.setAttribute('opacity', '0.5');
        }
        svg.appendChild(path);
    });
}

// Redraw arrows on scroll/resize
document.getElementById('page-canvas').addEventListener('scroll', drawArrows);
window.addEventListener('resize', drawArrows);

// ── Sidebar toggle ─────────────────────────────────────────────────────────────
document.getElementById('btn-toggle-sidebar').addEventListener('click', () => {
    sidebar.classList.toggle('collapsed');
    setTimeout(drawArrows, 200); // Wait for CSS transition
});
document.getElementById('sidebar-close').addEventListener('click', () => {
    sidebar.classList.add('collapsed');
    drawArrows();
});

// ── Tracked changes ────────────────────────────────────────────────────────────
document.getElementById('btn-set-base').addEventListener('click', async () => {
    await flush();
    baseMarkdown = currentMarkdown;
    alert('Base state saved. Make edits, then click "Show Changes".');
});

document.getElementById('btn-diff-toggle').addEventListener('click', async () => {
    isDiffMode = !isDiffMode;
    const btn  = document.getElementById('btn-diff-toggle');

    if (isDiffMode) {
        await flush();
        editor.contentEditable = 'false';
        setStatus('Diff mode');
        btn.querySelector('span:last-child').textContent = 'Hide Changes';
        try {
            // generate_diff returns HTML instead of broken MD now
            const diffHtml = await invoke('generate_diff', {
                old_md: baseMarkdown,
                new_md: currentMarkdown
            });
            setEditorContent(diffHtml);
            drawArrows();
        } catch(e) {
            console.error(e);
            isDiffMode = false;
            editor.contentEditable = 'true';
        }
    } else {
        editor.contentEditable = 'true';
        btn.querySelector('span:last-child').textContent = 'Show Changes';
        try {
            const html = await invoke('md_to_html', { markdown: currentMarkdown });
            setEditorContent(html);
            renderComments();
        } catch(e) { console.error(e); }
        setStatus('● Saved');
    }
});

// ── Page Count & Print Rulers ─────────────────────────────────────────────────
function updatePageCount() {
    const canvas = document.getElementById('page-canvas');
    const isWeb  = canvas.style.alignItems === 'stretch';
    if (isWeb) {
        pageCount.textContent = 'Web View';
        // Remove any existing rulers
        [...editor.querySelectorAll('.page-break-ruler')].forEach(r => r.remove());
        return;
    }
    // 1056px = 11 inches @ 96 DPI (US Letter)
    const PAGE_H = 1056;
    const pages  = Math.max(1, Math.ceil(editor.scrollHeight / PAGE_H));
    pageCount.textContent = `Page 1 of ${pages}`;

    // Only inject page-break rulers when enabled in settings
    [...editor.querySelectorAll('.page-break-ruler')].forEach(r => r.remove());
    if (!settings?.pageRulers) return;
    for (let p = 1; p < pages; p++) {
        // Find a safe injection point: walk editor's direct children, accumulating
        // heights, and insert the ruler after the child that crosses the boundary.
        let cumH = 0;
        const boundary = p * PAGE_H;
        const children = [...editor.children].filter(c => !c.classList.contains('page-break-ruler'));
        for (let i = 0; i < children.length; i++) {
            cumH += children[i].offsetHeight;
            if (cumH >= boundary) {
                const ruler = document.createElement('div');
                ruler.className = 'page-break-ruler';
                ruler.setAttribute('contenteditable', 'false');
                ruler.textContent = `Page ${p + 1}`;
                children[i].after(ruler);
                break;
            }
        }
    }
}

// ── Zoom ────────────────────────────────────────────────────────────────────
window.zoomPage = (pct) => {
    document.getElementById('page-canvas').style.zoom = (pct / 100);
    updatePageCount();
};

// ── Sync loop ────────────────────────────────────────────────────────────────
editor.addEventListener('input', () => {
    if (isDiffMode) return;
    setStatus('Saving…', 'syncing');
    updatePageCount();
    updateWordCount();
    renderOutline();
    clearTimeout(syncTimer);
    const delay = settings?.autosaveMs ?? 900;
    if (delay > 0) syncTimer = setTimeout(flush, delay);
});

// Update page count on load
new ResizeObserver(updatePageCount).observe(editor);

async function flush() {
    if (isDiffMode) return;
    try {
        // Embed comment metadata block
        const metadataHtml = `<script type="application/vnd.marksmen.comments">${JSON.stringify(window.marksmenComments)}</script>`;
        const payload = editor.innerHTML + '\n' + metadataHtml;
        currentMarkdown = await invoke('html_to_md', { html: payload });
        setStatus('● Saved');
    } catch(e) {
        setStatus('Sync error', 'error');
        console.error(e);
    }
}

function setStatus(text, cls = '') {
    syncStatus.textContent = text;
    syncStatus.className   = 'sync-status' + (cls ? ' ' + cls : '');
    if (cls) {
        setTimeout(() => {
            if (syncStatus.textContent === text)
                setStatus('● Saved');
        }, 4000);
    }
}

// ── Word Count ────────────────────────────────────────────────────────────────────
function updateWordCount() {
    const text  = editor.innerText || '';
    const words = text.trim() === '' ? 0 : text.trim().split(/\s+/).length;
    const chars = text.length;
    wordCount.textContent = `${words.toLocaleString()} words · ${chars.toLocaleString()} chars`;
}

// ── Document Outline ──────────────────────────────────────────────────────────────────
function renderOutline() {
    const headings = [...editor.querySelectorAll('h1,h2,h3,h4,h5,h6')]
        .filter(h => !h.classList.contains('page-break-ruler'));
    if (headings.length === 0) {
        outlineList.innerHTML = '<div class="sidebar-empty">No headings found.<br>Add headings (H1–H6) to build an outline.</div>';
        return;
    }
    outlineList.innerHTML = '';
    headings.forEach(h => {
        const level = parseInt(h.tagName[1], 10);
        const item  = document.createElement('div');
        item.className = `outline-item outline-item--h${level}`;
        const badge = document.createElement('span');
        badge.className = 'outline-badge';
        badge.textContent = `H${level}`;
        const label = document.createElement('span');
        label.textContent = h.innerText.trim().slice(0, 60);
        item.appendChild(badge);
        item.appendChild(label);
        item.addEventListener('click', () => {
            h.scrollIntoView({ behavior: 'smooth', block: 'start' });
            h.style.outline = '2px solid var(--accent)';
            setTimeout(() => { h.style.outline = ''; }, 1500);
        });
        outlineList.appendChild(item);
    });
}

// ── Sidebar Tabs ───────────────────────────────────────────────────────────────────
function switchSidebarTab(tab) {
    const isComments = tab === 'comments';
    document.getElementById('stab-comments').classList.toggle('stab--active', isComments);
    document.getElementById('stab-outline').classList.toggle('stab--active', !isComments);
    commentList.hidden = !isComments;
    outlineList.hidden = isComments;
    if (!isComments) renderOutline();
}
document.getElementById('stab-comments').addEventListener('click', () => switchSidebarTab('comments'));
document.getElementById('stab-outline').addEventListener('click',  () => switchSidebarTab('outline'));

// ── Find & Replace Engine ─────────────────────────────────────────────────────────────
let findMatches   = [];
let findCursor    = -1;
const FIND_CLASS  = 'find-highlight';
const FIND_ACTIVE = 'find-highlight--active';

function openFind(withReplace = false) {
    findBar.hidden = false;
    document.getElementById('replace-fields').hidden = !withReplace;
    document.getElementById('find-input').focus();
    document.getElementById('find-input').select();
}
function closeFind() {
    findBar.hidden = true;
    clearFindHighlights();
    editor.focus();
}
function clearFindHighlights() {
    [...editor.querySelectorAll('.' + FIND_CLASS)].forEach(span => {
        const parent = span.parentNode;
        while (span.firstChild) parent.insertBefore(span.firstChild, span);
        parent.removeChild(span);
        parent.normalize();
    });
    findMatches = [];
    findCursor  = -1;
    document.getElementById('find-count').textContent = '';
}
function runFind() {
    clearFindHighlights();
    const query    = document.getElementById('find-input').value;
    const useCase  = document.getElementById('find-case').checked;
    const useRegex = document.getElementById('find-regex').checked;
    if (!query) return;

    let pattern;
    try {
        pattern = new RegExp(useRegex ? query : query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'),
            useCase ? 'g' : 'gi');
    } catch { return; }

    // Walk text nodes in the editor and wrap matches
    const walker = document.createTreeWalker(
        editor, NodeFilter.SHOW_TEXT,
        { acceptNode: n => n.parentElement.closest('.page-break-ruler') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT }
    );
    const textNodes = [];
    while (walker.nextNode()) textNodes.push(walker.currentNode);

    textNodes.forEach(node => {
        const text = node.nodeValue;
        const parts = [];
        let last = 0, m;
        pattern.lastIndex = 0;
        while ((m = pattern.exec(text)) !== null) {
            if (m.index > last) parts.push(document.createTextNode(text.slice(last, m.index)));
            const span = document.createElement('mark');
            span.className = FIND_CLASS;
            span.textContent = m[0];
            parts.push(span);
            findMatches.push(span);
            last = m.index + m[0].length;
        }
        if (parts.length === 0) return;
        if (last < text.length) parts.push(document.createTextNode(text.slice(last)));
        const frag = document.createDocumentFragment();
        parts.forEach(p => frag.appendChild(p));
        node.parentNode.replaceChild(frag, node);
    });

    const total = findMatches.length;
    if (total > 0) {
        findCursor = 0;
        activateFindMatch(0);
    }
    document.getElementById('find-count').textContent = total > 0 ? `1 of ${total}` : 'No results';
}
function activateFindMatch(idx) {
    findMatches.forEach((m, i) => m.classList.toggle(FIND_ACTIVE, i === idx));
    if (findMatches[idx]) {
        findMatches[idx].scrollIntoView({ behavior: 'smooth', block: 'center' });
        document.getElementById('find-count').textContent = `${idx + 1} of ${findMatches.length}`;
    }
}
function findStep(dir) {
    if (findMatches.length === 0) return;
    findCursor = (findCursor + dir + findMatches.length) % findMatches.length;
    activateFindMatch(findCursor);
}
function replaceOne() {
    if (!findMatches[findCursor]) return;
    const val = document.getElementById('replace-input').value;
    findMatches[findCursor].outerHTML = document.createTextNode(val).nodeValue
        .replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
    findMatches.splice(findCursor, 1);
    if (findMatches.length > 0) {
        findCursor = Math.min(findCursor, findMatches.length - 1);
        activateFindMatch(findCursor);
    } else {
        document.getElementById('find-count').textContent = 'No results';
    }
    editor.dispatchEvent(new Event('input'));
}
function replaceAll() {
    const val = document.getElementById('replace-input').value;
    [...findMatches].forEach(m => { m.outerHTML = val; });
    findMatches = [];
    findCursor  = -1;
    document.getElementById('find-count').textContent = 'Replaced';
    editor.dispatchEvent(new Event('input'));
}

document.getElementById('find-input').addEventListener('input', runFind);
document.getElementById('find-input').addEventListener('keydown', e => {
    if (e.key === 'Enter')  { e.preventDefault(); findStep(e.shiftKey ? -1 : 1); }
    if (e.key === 'Escape') { e.preventDefault(); closeFind(); }
});
document.getElementById('find-case').addEventListener('change',  runFind);
document.getElementById('find-regex').addEventListener('change', runFind);
document.getElementById('find-next').addEventListener('click',  () => findStep(1));
document.getElementById('find-prev').addEventListener('click',  () => findStep(-1));
document.getElementById('replace-one').addEventListener('click', replaceOne);
document.getElementById('replace-all').addEventListener('click', replaceAll);
document.getElementById('find-replace-toggle').addEventListener('click', () => {
    const fields = document.getElementById('replace-fields');
    fields.hidden = !fields.hidden;
});
document.getElementById('find-close').addEventListener('click', closeFind);

// ── Keyboard Shortcuts ──────────────────────────────────────────────────────────────────
document.addEventListener('keydown', e => {
    const ctrl = e.ctrlKey || e.metaKey;
    if (ctrl && e.key === 'f') { e.preventDefault(); openFind(false); }
    if (ctrl && e.key === 'h') { e.preventDefault(); openFind(true); }
    if (ctrl && e.key === 's') { e.preventDefault(); flush(); }
    if (e.key === 'Escape' && !findBar.hidden) { e.preventDefault(); closeFind(); }
    if (e.key === 'Escape') { closeCtxMenu(); }
    if (ctrl && e.key === 'p') { e.preventDefault(); window.print(); }
    if (ctrl && e.key === ',') { e.preventDefault(); openSettings(); }
});

// ── Settings Engine ──────────────────────────────────────────────────────────────────
const SETTINGS_KEY = 'marksmen-settings-v1';
const SETTINGS_DEFAULTS = {
    author:      'You',
    theme:       'dark',
    fontFamily:  'Inter, sans-serif',
    fontSize:    14,
    lineHeight:  1.6,
    pageSize:    816,
    spellCheck:  true,
    pageRulers:  true,
    autosaveMs:  900,
};

let settings = { ...SETTINGS_DEFAULTS };

function loadSettings() {
    try {
        const stored = localStorage.getItem(SETTINGS_KEY);
        if (stored) settings = { ...SETTINGS_DEFAULTS, ...JSON.parse(stored) };
    } catch { /* corrupt — use defaults */ }
}
function saveSettings() {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
}
function applySettings() {
    // Theme
    if (settings.theme === 'dark') {
        document.body.classList.add('dark-theme');
    } else if (settings.theme === 'light') {
        document.body.classList.remove('dark-theme');
    } else {
        // system
        const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        document.body.classList.toggle('dark-theme', prefersDark);
    }
    // Font
    editor.style.fontFamily = settings.fontFamily;
    editor.style.fontSize   = settings.fontSize + 'px';
    editor.style.lineHeight = settings.lineHeight;
    // Page width
    document.documentElement.style.setProperty('--page-w', settings.pageSize + 'px');
    editor.style.width = 'var(--page-w)';
    // Spell check
    editor.setAttribute('spellcheck', settings.spellCheck ? 'true' : 'false');
    // Page rulers visibility — handled in updatePageCount via settings.pageRulers flag
    // Autosave — rebuild the flush timeout; existing timer keeps firing at old rate
    // (harmless; next edit will pick up new interval)
}
function populateSettingsUI() {
    document.getElementById('s-author').value      = settings.author;
    document.getElementById('s-theme').value       = settings.theme;
    document.getElementById('s-font-family').value = settings.fontFamily;
    document.getElementById('s-font-size').value   = settings.fontSize;
    document.getElementById('s-line-height').value = settings.lineHeight.toString();
    document.getElementById('s-page-size').value   = settings.pageSize.toString();
    document.getElementById('s-spell').checked     = settings.spellCheck;
    document.getElementById('s-rulers').checked    = settings.pageRulers;
    document.getElementById('s-autosave').value    = settings.autosaveMs.toString();
}
function readSettingsUI() {
    settings.author      = document.getElementById('s-author').value.trim() || 'You';
    settings.theme       = document.getElementById('s-theme').value;
    settings.fontFamily  = document.getElementById('s-font-family').value;
    settings.fontSize    = parseInt(document.getElementById('s-font-size').value, 10) || 14;
    settings.lineHeight  = parseFloat(document.getElementById('s-line-height').value) || 1.6;
    settings.pageSize    = parseInt(document.getElementById('s-page-size').value, 10) || 816;
    settings.spellCheck  = document.getElementById('s-spell').checked;
    settings.pageRulers  = document.getElementById('s-rulers').checked;
    settings.autosaveMs  = parseInt(document.getElementById('s-autosave').value, 10);
    saveSettings();
    applySettings();
    updatePageCount();
}
function openSettings() {
    populateSettingsUI();
    document.getElementById('settings-panel').classList.add('settings-panel--open');
    document.getElementById('settings-scrim').classList.add('settings-scrim--open');
}
function closeSettingsPanel() {
    document.getElementById('settings-panel').classList.remove('settings-panel--open');
    document.getElementById('settings-scrim').classList.remove('settings-scrim--open');
}

document.getElementById('btn-settings').addEventListener('click', openSettings);
document.getElementById('settings-close').addEventListener('click', closeSettingsPanel);
document.getElementById('settings-scrim').addEventListener('click', closeSettingsPanel);
document.getElementById('settings-reset').addEventListener('click', () => {
    settings = { ...SETTINGS_DEFAULTS };
    saveSettings();
    populateSettingsUI();
    applySettings();
    updatePageCount();
});
// Live-apply on each change
['s-author','s-theme','s-font-family','s-font-size','s-line-height',
 's-page-size','s-spell','s-rulers','s-autosave'].forEach(id => {
    const el = document.getElementById(id);
    el.addEventListener('change', readSettingsUI);
    if (el.type === 'text') el.addEventListener('input', readSettingsUI);
});

// ── Recent Files ─────────────────────────────────────────────────────────────────────
const RECENT_KEY    = 'marksmen-recent-v1';
const RECENT_MAX    = 8;

function loadRecentFiles() {
    try { return JSON.parse(localStorage.getItem(RECENT_KEY) || '[]'); } catch { return []; }
}
function pushRecentFile(displayName) {
    let list = loadRecentFiles().filter(r => r !== displayName);
    list.unshift(displayName);
    if (list.length > RECENT_MAX) list = list.slice(0, RECENT_MAX);
    localStorage.setItem(RECENT_KEY, JSON.stringify(list));
    renderRecentFiles();
}
function renderRecentFiles() {
    const list = loadRecentFiles();
    const section = document.getElementById('recent-files-section');
    const container = document.getElementById('recent-files-list');
    if (list.length === 0) { section.hidden = true; return; }
    section.hidden = false;
    container.innerHTML = '';
    list.forEach(name => {
        const btn = document.createElement('button');
        btn.className = 'fmenu-recent-item';
        btn.innerHTML = `<span style="font-size:14px">&#128196;</span><span class="fmenu-recent-name">${name}</span>`;
        // Recent files are display-name only — clicking them re-opens the OS dialog
        // pre-filtered to that name; in a future phase, file paths will be stored.
        btn.title = `Reopen ${name}`;
        container.appendChild(btn);
    });
}

// Wire pushRecentFile into the open handler — patch: intercept after import
const _origOpenBtn = document.getElementById('btn-open');
_origOpenBtn.addEventListener('click', () => {
    // The push happens after currentDocName is set in the existing click handler.
    // We listen for the custom 'marksmen:opened' event dispatched below.
}, { capture: true });
document.addEventListener('marksmen:opened', e => {
    if (e.detail?.name) pushRecentFile(e.detail.name);
});

// \u2500\u2500 Table Operations Engine \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500
const tableFloatBar = document.getElementById('table-float-bar');
let activeTable = null;

function closeFloatBar() { tableFloatBar.hidden = true; }

function makeCell(tag, content = ' ') {
    const el = document.createElement(tag); el.textContent = content; return el;
}
function tableSync() { editor.dispatchEvent(new Event('input')); }

// Row ops
function addRowAbove(cell) {
    const row = cell.closest('tr');
    const newRow = document.createElement('tr');
    [...row.children].forEach(c => newRow.appendChild(makeCell(c.tagName.toLowerCase())));
    row.parentElement.insertBefore(newRow, row); tableSync();
}
function addRowBelow(cell) {
    const row = cell.closest('tr');
    const newRow = document.createElement('tr');
    [...row.children].forEach(c => newRow.appendChild(makeCell(c.tagName.toLowerCase())));
    row.parentElement.insertBefore(newRow, row.nextSibling); tableSync();
}
function deleteRow(cell) {
    const row = cell.closest('tr'), tbody = row.parentElement;
    row.remove(); if (tbody.children.length === 0) tbody.remove(); tableSync();
}
function clearRow(cell) {
    [...cell.closest('tr').children].forEach(c => { c.textContent = ' '; }); tableSync();
}

// Column ops
function colIdx(cell) { return [...cell.parentElement.children].indexOf(cell); }
function addColLeft(cell) {
    const table = cell.closest('table'), idx = colIdx(cell);
    [...table.querySelectorAll('tr')].forEach(r =>
        r.insertBefore(makeCell(r.children[idx]?.tagName?.toLowerCase() || 'td'), r.children[idx]));
    tableSync();
}
function addColRight(cell) {
    const table = cell.closest('table'), idx = colIdx(cell);
    [...table.querySelectorAll('tr')].forEach(r =>
        r.insertBefore(makeCell(r.children[idx]?.tagName?.toLowerCase() || 'td'), r.children[idx + 1] || null));
    tableSync();
}
function deleteCol(cell) {
    const table = cell.closest('table'), idx = colIdx(cell);
    [...table.querySelectorAll('tr')].forEach(r => { if (r.children[idx]) r.children[idx].remove(); });
    tableSync();
}
function clearCol(cell) {
    const table = cell.closest('table'), idx = colIdx(cell);
    [...table.querySelectorAll('tr')].forEach(r => { if (r.children[idx]) r.children[idx].textContent = ' '; });
    tableSync();
}
function deleteTable(cell) { cell.closest('table').remove(); tableSync(); }

// Multi-select (Shift+click)
let selectedCells = new Set();
function clearCellSelection() {
    selectedCells.forEach(c => c.classList.remove('cell-selected')); selectedCells.clear();
}
editor.addEventListener('mousedown', e => {
    const cell = e.target.closest('td, th');
    if (!cell) { clearCellSelection(); return; }
    if (e.shiftKey) {
        e.preventDefault();
        if (selectedCells.has(cell)) { cell.classList.remove('cell-selected'); selectedCells.delete(cell); }
        else { cell.classList.add('cell-selected'); selectedCells.add(cell); }
    } else {
        clearCellSelection();
        cell.classList.add('cell-selected'); selectedCells.add(cell);
    }
});

// Merge (horizontal same-row or vertical same-column)
function mergeSelectedCells() {
    if (selectedCells.size < 2) return;
    const cells = [...selectedCells];
    const rows  = new Set(cells.map(c => c.closest('tr')));
    if (rows.size === 1) {
        cells.sort((a, b) => colIdx(a) - colIdx(b));
        const first = cells[0];
        first.innerHTML = cells.map(c => c.innerHTML).join(' ');
        first.colSpan   = cells.length;
        cells.slice(1).forEach(c => c.remove());
    } else {
        const cols = new Set(cells.map(colIdx));
        if (cols.size === 1) {
            const allRows = [...cells[0].closest('table').querySelectorAll('tr')];
            cells.sort((a, b) => allRows.indexOf(a.closest('tr')) - allRows.indexOf(b.closest('tr')));
            const first = cells[0];
            first.innerHTML = cells.map(c => c.innerHTML).join(' ');
            first.rowSpan   = cells.length;
            cells.slice(1).forEach(c => c.remove());
        }
    }
    clearCellSelection(); tableSync();
}

// Split merged cell
function splitCell(cell) {
    const cs = parseInt(cell.colSpan || 1, 10);
    const rs = parseInt(cell.rowSpan || 1, 10);
    if (cs > 1) {
        cell.colSpan = 1;
        for (let i = 1; i < cs; i++)
            cell.parentElement.insertBefore(makeCell(cell.tagName.toLowerCase()), cell.nextSibling);
    }
    if (rs > 1) {
        cell.rowSpan = 1;
        const rows = [...cell.closest('table').querySelectorAll('tr')];
        const ri = rows.indexOf(cell.closest('tr')), ci = colIdx(cell);
        for (let i = 1; i < rs; i++) {
            const tr = rows[ri + i]; if (!tr) break;
            tr.insertBefore(makeCell(cell.tagName.toLowerCase()), tr.children[ci] || null);
        }
    }
    tableSync();
}

// Alignment
function alignCells(align) {
    const targets = selectedCells.size > 0 ? [...selectedCells] : (ctxTargetCell ? [ctxTargetCell] : []);
    targets.forEach(c => { c.style.textAlign = align; }); tableSync();
}

// Select entire table
function selectTable(cell) {
    const table = cell.closest('table');
    clearCellSelection();
    [...table.querySelectorAll('td, th')].forEach(c => { c.classList.add('cell-selected'); selectedCells.add(c); });
}

// Context menu wiring
editor.addEventListener('contextmenu', e => {
    const cell = e.target.closest('td, th');
    if (!cell) { closeCtxMenu(); return; }
    e.preventDefault();
    ctxTargetCell = cell;
    if (!selectedCells.has(cell)) { clearCellSelection(); cell.classList.add('cell-selected'); selectedCells.add(cell); }
    document.getElementById('ctx-merge-cells').style.display = selectedCells.size >= 2 ? '' : 'none';
    document.getElementById('ctx-split-cell').style.display  = (cell.colSpan > 1 || cell.rowSpan > 1) ? '' : 'none';
    ctxMenu.style.left = e.clientX + 'px';
    ctxMenu.style.top  = e.clientY + 'px';
    ctxMenu.hidden = false;
});
document.addEventListener('click', e => {
    if (!ctxMenu.hidden && !ctxMenu.contains(e.target)) closeCtxMenu();
});

const ctxWire = (id, fn) => document.getElementById(id)?.addEventListener('click', () => { fn(); closeCtxMenu(); });
ctxWire('ctx-add-row-above', () => ctxTargetCell && addRowAbove(ctxTargetCell));
ctxWire('ctx-add-row-below', () => ctxTargetCell && addRowBelow(ctxTargetCell));
ctxWire('ctx-add-col-left',  () => ctxTargetCell && addColLeft(ctxTargetCell));
ctxWire('ctx-add-col-right', () => ctxTargetCell && addColRight(ctxTargetCell));
ctxWire('ctx-merge-cells',   () => mergeSelectedCells());
ctxWire('ctx-split-cell',    () => ctxTargetCell && splitCell(ctxTargetCell));
ctxWire('ctx-clear-row',     () => ctxTargetCell && clearRow(ctxTargetCell));
ctxWire('ctx-clear-col',     () => ctxTargetCell && clearCol(ctxTargetCell));
ctxWire('ctx-select-table',  () => ctxTargetCell && selectTable(ctxTargetCell));
ctxWire('ctx-del-row',       () => ctxTargetCell && deleteRow(ctxTargetCell));
ctxWire('ctx-del-col',       () => ctxTargetCell && deleteCol(ctxTargetCell));
ctxWire('ctx-del-table',     () => ctxTargetCell && deleteTable(ctxTargetCell));

// Floating toolbar
function positionFloatBar(table) {
    const rect = table.getBoundingClientRect();
    tableFloatBar.style.left = Math.max(0, rect.left) + 'px';
    tableFloatBar.style.top  = Math.max(0, rect.top - 46) + 'px';
    tableFloatBar.hidden = false;
}
function updateFloatBar() {
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0) { closeFloatBar(); return; }
    const node  = sel.getRangeAt(0).startContainer;
    const table = node.nodeType === 1
        ? node.closest?.('table') : node.parentElement?.closest('table');
    if (table) {
        activeTable   = table;
        ctxTargetCell = node.nodeType === 1
            ? node.closest?.('td,th') : node.parentElement?.closest('td,th');
        positionFloatBar(table);
    } else { closeFloatBar(); activeTable = null; }
}
editor.addEventListener('keyup',   updateFloatBar);
editor.addEventListener('mouseup', updateFloatBar);

const tfbWire = (id, fn) => document.getElementById(id)?.addEventListener('click', fn);
tfbWire('tfb-add-row-above', () => ctxTargetCell && addRowAbove(ctxTargetCell));
tfbWire('tfb-add-row-below', () => ctxTargetCell && addRowBelow(ctxTargetCell));
tfbWire('tfb-add-col-left',  () => ctxTargetCell && addColLeft(ctxTargetCell));
tfbWire('tfb-add-col-right', () => ctxTargetCell && addColRight(ctxTargetCell));
tfbWire('tfb-merge',         () => mergeSelectedCells());
tfbWire('tfb-split',         () => ctxTargetCell && splitCell(ctxTargetCell));
tfbWire('tfb-align-left',    () => alignCells('left'));
tfbWire('tfb-align-center',  () => alignCells('center'));
tfbWire('tfb-align-right',   () => alignCells('right'));
tfbWire('tfb-del-row',       () => { ctxTargetCell && deleteRow(ctxTargetCell); });
tfbWire('tfb-del-col',       () => { ctxTargetCell && deleteCol(ctxTargetCell); });
tfbWire('tfb-del-table',     () => { activeTable && (activeTable.remove(), tableSync(), closeFloatBar()); });


// ── Initial render ───────────────────────────────────────────────────────────────────
loadSettings();
applySettings();
renderRecentFiles();
updateWordCount();
renderOutline();
flush();

// \u2500\u2500 Phase 19 \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500

// \u2500\u2500 Bottom Status Bar \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500
const sbarCaret   = document.getElementById('sbar-caret');
const sbarReading = document.getElementById('sbar-reading');
const sbarZoom    = document.getElementById('sbar-zoom');
const sbarZoomLbl = document.getElementById('sbar-zoom-label');

function updateStatusBar() {
    // Reading time: ~200 words/min average
    const text  = editor.innerText || '';
    const words = text.trim() === '' ? 0 : text.trim().split(/\s+/).length;
    const mins  = Math.max(1, Math.ceil(words / 200));
    sbarReading.textContent = `${mins} min read`;

    // Caret position (line / col inside the editor)
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0) return;
    const range = sel.getRangeAt(0).cloneRange();
    range.collapse(true);
    // Walk the editor to count lines: place a temporary range from start of editor
    const preRange = document.createRange();
    preRange.selectNodeContents(editor);
    preRange.setEnd(range.startContainer, range.startOffset);
    const preText = preRange.toString();
    const line = (preText.match(/\n/g) || []).length + 1;
    const col  = preText.length - preText.lastIndexOf('\n');
    sbarCaret.textContent = `Ln ${line}, Col ${col}`;
}

editor.addEventListener('keyup',   updateStatusBar);
editor.addEventListener('mouseup', updateStatusBar);
editor.addEventListener('input',   updateStatusBar);

sbarZoom.addEventListener('input', () => {
    const pct = parseInt(sbarZoom.value, 10);
    sbarZoomLbl.textContent = pct + '%';
    document.getElementById('page-canvas').style.zoom = pct / 100;
    updatePageCount();
});

// \u2500\u2500 Heading Keyboard Shortcuts (Ctrl+1\u20136) \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500
// These are additive to the global keydown listener already registered above.
// Ctrl+Alt+1-6 mirrors Word/Google Docs conventions.
document.addEventListener('keydown', e => {
    if (!e.altKey || !(e.ctrlKey || e.metaKey)) return;
    const lvl = parseInt(e.key, 10);
    if (lvl < 1 || lvl > 6) return;
    e.preventDefault();
    const tag = lvl === 0 ? 'p' : `h${lvl}`;
    document.execCommand('formatBlock', false, `<${tag}>`);
    editor.dispatchEvent(new Event('input'));
});

// \u2500\u2500 Link Insert Handler \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500
document.getElementById('btn-insert-link')?.addEventListener('click', () => {
    const sel  = window.getSelection();
    const text = sel && !sel.isCollapsed ? sel.toString() : '';
    const url  = prompt('Enter URL:', 'https://');
    if (!url) return;
    const label = text || url;
    // Preserve existing selection to wrap, or insert at cursor
    if (sel && !sel.isCollapsed) {
        document.execCommand('createLink', false, url);
    } else {
        document.execCommand('insertHTML', false,
            `<a href="${url}" target="_blank" rel="noopener noreferrer">${label}</a>`);
    }
    editor.dispatchEvent(new Event('input'));
});

// \u2500\u2500 Theme Toggle \u2192 sync with settings panel \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500
// The Ribbon "Dark Mode" toggle should mirror and persist through settings.
const ribbonThemeBtn = document.getElementById('btn-toggle-theme');
if (ribbonThemeBtn) {
    ribbonThemeBtn.addEventListener('click', () => {
        const isDark = document.body.classList.toggle('dark-theme');
        settings.theme = isDark ? 'dark' : 'light';
        saveSettings();
        // Sync the settings panel select if it's open
        const sel = document.getElementById('s-theme');
        if (sel) sel.value = settings.theme;
    });
}

// \u2500\u2500 System theme watcher \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', e => {
    if (settings.theme === 'system') {
        document.body.classList.toggle('dark-theme', e.matches);
    }
});
