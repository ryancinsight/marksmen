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
                if (cmd === 'export_format') {
                    if (args?.format === 'rtf') return ['e30=', 'application/rtf', 'document.rtf'];
                    return ['bW9jaw==', 'text/plain', 'document.txt'];
                }
                if (cmd === 'save_file') { console.log('[Mock] save_file'); return '/mock/path/document.md'; }
                if (cmd === 'save_as_format') { console.log('[Mock] save_as_format'); return; }
                if (cmd === 'print_pdf') { console.log('[Mock] print_pdf'); return; }
                if (cmd === 'open_file_by_path') return ['# Reopened\n\nMock reopen.\n', args?.path?.split('/').pop() || 'file.md'];
                throw new Error('Unknown mock command: ' + cmd);
            }
        }
    };
}
const { invoke } = window.__TAURI__.core;

// ── Window-size lock ─────────────────────────────────────────────────────────
// .app-shell uses position:fixed; inset:0 so it is always anchored directly
// to the WebView2 viewport — no document-height influence possible.
// ── Custom Undo Stack ────────────────────────────────────────────────────────
const UNDO_LIMIT = 50;
let undoStack = [];
let redoStack = [];
let isUndoing = false;

function snapshotState() {
    if (isUndoing || isDiffMode) return;
    const currentHtml = editor.innerHTML;
    if (undoStack.length > 0 && undoStack[undoStack.length - 1] === currentHtml) return;
    undoStack.push(currentHtml);
    if (undoStack.length > UNDO_LIMIT) undoStack.shift();
    redoStack = []; // Clear redo on new action
}
// Alias used throughout Phase 24-27 code. Also exposed on window for references.js.
const pushUndoSnapshot = snapshotState;
window.pushUndoSnapshot = snapshotState;

function performUndo() {
    if (undoStack.length > 1) { // Leave at least the initial state
        isUndoing = true;
        const currentHtml = undoStack.pop();
        redoStack.push(currentHtml);
        editor.innerHTML = undoStack[undoStack.length - 1];
        isUndoing = false;
        flush();
    }
}

function performRedo() {
    if (redoStack.length > 0) {
        isUndoing = true;
        const nextHtml = redoStack.pop();
        undoStack.push(nextHtml);
        editor.innerHTML = nextHtml;
        isUndoing = false;
        flush();
    }
}

// setEditorContent wraps innerHTML assignment to keep a single call-site.
function setEditorContent(html) {
    if (window.stateObserver) window.stateObserver.disconnect();
    editor.innerHTML = html;
    undoStack = [html];
    redoStack = [];
    isDirty = false;
    if (window.stateObserver) window.stateObserver.observe(editor, { childList: true, characterData: true, subtree: true });
}

// ── Undo/Redo Keybindings ───────────────────────────────────────────────────
document.addEventListener('keydown', e => {
    if (e.ctrlKey || e.metaKey) {
        if (e.key === 'z') {
            e.preventDefault();
            performUndo();
        } else if (e.key === 'Z' || (e.key === 'y' && !e.shiftKey)) {
            // Ctrl+Shift+Z or Ctrl+Y
            e.preventDefault();
            performRedo();
        } else if (e.shiftKey && e.key === 'E') {
            // Ctrl+Shift+E — toggle Track Changes
            e.preventDefault();
            toggleTrackChanges();
        }
    }
});


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
let currentFilePath = null;  // absolute path of the open file, null = unsaved
let isDirty         = false; // true if there are unsaved changes
window.marksmenComments = {};
let isTrackChanges = false; // Phase 26: Track Changes mode

// ── Theme toggle ─────────────────────────────────────────────────────────────
document.getElementById('btn-toggle-theme')?.addEventListener('click', () => {
    document.body.classList.toggle('dark-theme');
});

// ── Ribbon tab switching ─────────────────────────────────────────────────────
document.querySelectorAll('.rtab[data-panel]').forEach(tab => {
    tab.addEventListener('click', () => {
        document.querySelectorAll('.rtab[data-panel]').forEach(t => t.classList.remove('rtab--active'));
        document.querySelectorAll('.ribbon-panel').forEach(p => p.classList.remove('ribbon-panel--active'));
        tab.classList.add('rtab--active');
        document.getElementById(tab.dataset.panel)?.classList.add('ribbon-panel--active');
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
    currentFilePath  = null;
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
        const [md, filename, absolutePath] = await invoke('import_file');
        currentMarkdown = md;
        baseMarkdown    = md;
        currentFilePath = absolutePath;
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
        document.dispatchEvent(new CustomEvent('marksmen:opened', { detail: { name: currentDocName, path: currentFilePath } }));

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
// mousedown + preventDefault keeps focus in the editor so the command applies
// to the current selection (or arms the format for the next characters typed).
document.querySelectorAll('[data-cmd]').forEach(btn => {
    btn.addEventListener('mousedown', e => {
        e.preventDefault();           // do NOT blur the editor
        const { cmd, val } = btn.dataset;
        document.execCommand(cmd, false, val || null);
        // Return focus explicitly so caret stays active for pre-emptive typing
        editor.focus();
        editor.dispatchEvent(new Event('input'));
        // Update active-state indicators immediately (before selectionchange fires)
        updateFormatState();
    });
});

// ── Pre-emptive format state indicators ──────────────────────────────────────
// Maps execCommand name → ribbon button selector.
// queryCommandState() returns true when the format is active at the caret
// (either because text is selected in that style, or because the user just
// toggled it with a collapsed cursor and hasn't typed yet).
const FORMAT_STATE_MAP = [
    { cmd: 'bold',          sel: '[data-cmd="bold"]' },
    { cmd: 'italic',        sel: '[data-cmd="italic"]' },
    { cmd: 'underline',     sel: '[data-cmd="underline"]' },
    { cmd: 'strikeThrough', sel: '[data-cmd="strikeThrough"]' },
    { cmd: 'superscript',   sel: '[data-cmd="superscript"]' },
    { cmd: 'subscript',     sel: '[data-cmd="subscript"]' },
    { cmd: 'insertUnorderedList', sel: '[data-cmd="insertUnorderedList"]' },
    { cmd: 'insertOrderedList',   sel: '[data-cmd="insertOrderedList"]' },
    { cmd: 'justifyLeft',   sel: '[data-cmd="justifyLeft"]' },
    { cmd: 'justifyCenter', sel: '[data-cmd="justifyCenter"]' },
    { cmd: 'justifyRight',  sel: '[data-cmd="justifyRight"]' },
    { cmd: 'justifyFull',   sel: '[data-cmd="justifyFull"]' },
];

// Heading / block-format detection — queryCommandValue returns the current tag.
const BLOCK_FORMAT_MAP = [
    { val: 'h1',         sel: '[data-cmd="formatBlock"][data-val="h1"]' },
    { val: 'h2',         sel: '[data-cmd="formatBlock"][data-val="h2"]' },
    { val: 'h3',         sel: '[data-cmd="formatBlock"][data-val="h3"]' },
    { val: 'p',          sel: '[data-cmd="formatBlock"][data-val="p"]' },
    { val: 'blockquote', sel: '[data-cmd="formatBlock"][data-val="blockquote"]' },
    { val: 'pre',        sel: '[data-cmd="formatBlock"][data-val="pre"]' },
];

function updateFormatState() {
    // Only meaningful when editor has focus / is the selection host
    if (!editor.contains(document.getSelection()?.anchorNode) &&
        document.activeElement !== editor) return;

    FORMAT_STATE_MAP.forEach(({ cmd, sel }) => {
        const active = document.queryCommandState(cmd);
        document.querySelectorAll(sel).forEach(btn =>
            btn.classList.toggle('rbtn--active-toggle', active)
        );
    });

    // Block format: highlight the matching style button
    const blockVal = document.queryCommandValue('formatBlock').toLowerCase().replace(/[<>]/g, '');
    BLOCK_FORMAT_MAP.forEach(({ val, sel }) => {
        document.querySelectorAll(sel).forEach(btn =>
            btn.classList.toggle('rbtn--active-toggle', blockVal === val)
        );
    });

    // L08: Sync font-family and font-size pickers to caret position
    const sel = window.getSelection();
    const caretNode = sel?.anchorNode;
    if (caretNode) {
        const el = caretNode.nodeType === 3 ? caretNode.parentElement : caretNode;
        if (el && editor.contains(el)) {
            const cs = getComputedStyle(el);
            // Font family
            const ffPicker = document.getElementById('font-family-picker');
            if (ffPicker && document.activeElement !== ffPicker) {
                const ff = cs.fontFamily.split(',')[0].trim().replace(/["']/g, '');
                ffPicker.value = ff;
            }
            // Font size: convert px → pt (1px = 0.75pt)
            const fsPicker = document.getElementById('font-size-picker');
            if (fsPicker && document.activeElement !== fsPicker) {
                const fsPx = parseFloat(cs.fontSize);
                const fsPt = Math.round(fsPx * 0.75);
                // Find closest option
                const opts = [...fsPicker.options].map(o => parseInt(o.value, 10));
                const closest = opts.reduce((a, b) => Math.abs(b - fsPt) < Math.abs(a - fsPt) ? b : a, opts[0]);
                fsPicker.value = closest;
            }
        }
    }

    // Mirror active formats in the status bar (tiny badge list)
    const active = FORMAT_STATE_MAP
        .filter(({ cmd }) => document.queryCommandState(cmd))
        .map(({ cmd }) => cmd.replace('insert','').replace('Through','').replace('script',''));
    const badge = document.getElementById('sbar-format');
    if (badge) {
        badge.textContent = active.length > 0 ? active.join(' · ') : 'Markdown';
        badge.style.opacity = active.length > 0 ? '1' : '0.7';
    }

    // G-M30: Selection word count in status bar
    const wordCountEl = document.getElementById('sbar-word-count');
    if (wordCountEl) {
        const sel2 = window.getSelection();
        if (sel2 && !sel2.isCollapsed) {
            const txt = sel2.toString().trim();
            const wc = txt ? txt.split(/\s+/).filter(Boolean).length : 0;
            if (wc > 0) {
                const base = wordCountEl.textContent.replace(/\s*\(.*?selected.*?\)/, '').trim();
                wordCountEl.textContent = `${base} (${wc} selected)`;
            }
        } else {
            // Collapsed / no selection — strip the selection suffix if present
            const current = wordCountEl.textContent;
            if (current.includes('selected')) {
                wordCountEl.textContent = current.replace(/\s*\(.*?selected.*?\)/, '').trim();
            }
        }
    }
}

// Update format indicators whenever the caret moves or selection changes
document.addEventListener('selectionchange', () => {
    // Throttle to animation frame — selectionchange fires very frequently
    if (!updateFormatState._raf) {
        updateFormatState._raf = requestAnimationFrame(() => {
            updateFormatState._raf = null;
            updateFormatState();
        });
    }
});

// Also update on every editor keystroke (catches pre-emptive toggles mid-word)
editor.addEventListener('keyup', updateFormatState);

// Font family — input+datalist (H25/H26): triggers on 'change' (committed) and on 'input' (typing)
document.getElementById('font-family-picker').addEventListener('change', e => {
    const val = e.target.value.trim();
    if (val) { document.execCommand('fontName', false, val); editor.focus(); }
});
// Also fire when user presses Enter or selects from datalist
document.getElementById('font-family-picker').addEventListener('input', e => {
    const val = e.target.value.trim();
    // Only apply if the value is a known font (avoids applying on every keystroke mid-word)
    const dl = document.getElementById('font-family-list');
    const known = dl ? [...dl.options].map(o => o.value) : [];
    if (val && known.includes(val)) { document.execCommand('fontName', false, val); }
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

// ── Font grow / shrink ────────────────────────────────────────────────────────
document.getElementById('btn-font-grow')?.addEventListener('mousedown', e => {
    e.preventDefault();
    const sel = window.getSelection();
    const node = sel?.anchorNode?.parentElement || editor;
    const currentPt = parseFloat(getComputedStyle(node).fontSize) || 11;
    const newPt = Math.min(currentPt + 2, 96);
    if (sel && sel.rangeCount && !sel.isCollapsed) {
        const range = sel.getRangeAt(0);
        const span = document.createElement('span');
        span.style.fontSize = newPt + 'pt';
        try { range.surroundContents(span); } catch(_) {}
    }
    editor.focus();
    editor.dispatchEvent(new Event('input'));
});
document.getElementById('btn-font-shrink')?.addEventListener('mousedown', e => {
    e.preventDefault();
    const sel = window.getSelection();
    const node = sel?.anchorNode?.parentElement || editor;
    const currentPt = parseFloat(getComputedStyle(node).fontSize) || 11;
    const newPt = Math.max(currentPt - 2, 6);
    if (sel && sel.rangeCount && !sel.isCollapsed) {
        const range = sel.getRangeAt(0);
        const span = document.createElement('span');
        span.style.fontSize = newPt + 'pt';
        try { range.surroundContents(span); } catch(_) {}
    }
    editor.focus();
    editor.dispatchEvent(new Event('input'));
});

// ── Format Painter (H45) ─────────────────────────────────────────────────────
let _painterStyles = null;
let _painterLocked = false; // double-click = stay-on mode
const btnPainter = document.getElementById('btn-format-painter');
btnPainter?.addEventListener('mousedown', e => {
    e.preventDefault();
    const sel = window.getSelection();
    const node = sel?.anchorNode?.parentElement;
    if (!node) return;
    const cs = getComputedStyle(node);
    _painterStyles = {
        fontFamily:     cs.fontFamily,
        fontSize:       cs.fontSize,
        fontWeight:     cs.fontWeight,
        fontStyle:      cs.fontStyle,
        textDecoration: cs.textDecoration,
        color:          cs.color,
        backgroundColor: cs.backgroundColor,
    };
    // Double-click locks painter until Escape
    _painterLocked = e.detail >= 2;
    editor.style.cursor = 'copy';
    btnPainter.classList.add('rbtn--active-toggle');
});
editor.addEventListener('mouseup', () => {
    if (!_painterStyles) return;
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed) return;
    const range = sel.getRangeAt(0);
    const span = document.createElement('span');
    Object.assign(span.style, _painterStyles);
    try { range.surroundContents(span); } catch(_) {}
    editor.dispatchEvent(new Event('input'));
    if (!_painterLocked) {
        _painterStyles = null;
        editor.style.cursor = '';
        btnPainter.classList.remove('rbtn--active-toggle');
    }
    pushUndoSnapshot();
});
document.addEventListener('keydown', e => {
    if (e.key === 'Escape' && _painterStyles) {
        _painterStyles = null;
        _painterLocked = false;
        editor.style.cursor = '';
        btnPainter?.classList.remove('rbtn--active-toggle');
    }
});

// ── Change Case (H46) ────────────────────────────────────────────────────────
(function() {
    const btn    = document.getElementById('btn-change-case');
    const picker = document.getElementById('case-picker');
    if (!btn || !picker) return;

    function positionPicker(el, anchor) {
        const r = anchor.getBoundingClientRect();
        el.style.left = r.left + 'px';
        el.style.top  = (r.bottom + 2) + 'px';
        el.style.position = 'fixed';
        el.style.zIndex   = '9999';
    }

    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const isHidden = picker.hidden;
        picker.hidden = !isHidden;
        if (!isHidden) return;
        positionPicker(picker, btn);
        // Preserve editor selection before picker steals focus
        const sel = window.getSelection();
        if (sel && sel.rangeCount) window._savedCaseRange = sel.getRangeAt(0).cloneRange();
    });

    const CASES = {
        sentence: t => t.charAt(0).toUpperCase() + t.slice(1).toLowerCase(),
        lower:    t => t.toLowerCase(),
        upper:    t => t.toUpperCase(),
        title:    t => t.replace(/\w\S*/g, w => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase()),
        toggle:   t => [...t].map(c => c === c.toUpperCase() ? c.toLowerCase() : c.toUpperCase()).join(''),
    };

    picker.querySelectorAll('[data-case]').forEach(item => {
        item.addEventListener('mousedown', e => {
            e.preventDefault();
            const fn = CASES[item.dataset.case];
            if (!fn) return;
            editor.focus();
            const sel = window.getSelection();
            // Restore range if lost due to picker focus
            const range = window._savedCaseRange || (sel.rangeCount ? sel.getRangeAt(0) : null);
            if (!range) return;
            sel.removeAllRanges();
            sel.addRange(range);
            const text = sel.toString();
            if (text) document.execCommand('insertText', false, fn(text));
            picker.hidden = true;
            editor.dispatchEvent(new Event('input'));
            pushUndoSnapshot();
        });
    });

    // Shift+F3 cycles through cases
    let _caseIndex = 0;
    const _caseOrder = ['upper', 'lower', 'title', 'sentence'];
    document.addEventListener('keydown', e => {
        if (e.shiftKey && e.key === 'F3') {
            e.preventDefault();
            const sel = window.getSelection();
            if (!sel.rangeCount || sel.isCollapsed) return;
            editor.focus();
            const fn = CASES[_caseOrder[_caseIndex % _caseOrder.length]];
            document.execCommand('insertText', false, fn(sel.toString()));
            _caseIndex++;
            editor.dispatchEvent(new Event('input'));
        }
    });

    document.addEventListener('click', e => {
        if (!picker.hidden && !picker.contains(e.target) && e.target !== btn) picker.hidden = true;
    });
})();

// ── Underline style picker (H47) ─────────────────────────────────────────────
(function() {
    const btn    = document.getElementById('btn-underline-menu');
    const picker = document.getElementById('underline-picker');
    if (!btn || !picker) return;

    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        // Left portion = toggle underline; chevron = open picker
        const rect = btn.getBoundingClientRect();
        const isChevron = e.clientX > rect.right - 16;
        if (isChevron) {
            picker.hidden = !picker.hidden;
            if (!picker.hidden) {
                picker.style.position = 'fixed';
                picker.style.left = rect.left + 'px';
                picker.style.top  = (rect.bottom + 2) + 'px';
                picker.style.zIndex = '9999';
            }
        } else {
            // Simple toggle via execCommand
            document.execCommand('underline', false, null);
            editor.focus();
            editor.dispatchEvent(new Event('input'));
            updateFormatState();
        }
    });

    picker.querySelectorAll('[data-underline]').forEach(item => {
        item.addEventListener('mousedown', e => {
            e.preventDefault();
            const style = item.dataset.underline;
            const sel = window.getSelection();
            if (sel && sel.rangeCount && !sel.isCollapsed) {
                const range = sel.getRangeAt(0);
                const span = document.createElement('span');
                span.style.textDecoration = 'underline';
                if (style !== 'single') span.style.textDecorationStyle = style;
                try { range.surroundContents(span); } catch(_) {}
            }
            picker.hidden = true;
            editor.focus();
            editor.dispatchEvent(new Event('input'));
            pushUndoSnapshot();
        });
    });

    document.addEventListener('click', e => {
        if (!picker.hidden && !picker.contains(e.target) && e.target !== btn) picker.hidden = true;
    });
})();

// ── Text Effects picker (H48) ─────────────────────────────────────────────────
(function() {
    const btn    = document.getElementById('btn-text-effects');
    const picker = document.getElementById('effects-picker');
    if (!btn || !picker) return;

    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        picker.hidden = !picker.hidden;
        if (!picker.hidden) {
            const rect = btn.getBoundingClientRect();
            picker.style.position = 'fixed';
            picker.style.left = rect.left + 'px';
            picker.style.top  = (rect.bottom + 2) + 'px';
            picker.style.zIndex = '9999';
        }
    });

    picker.querySelectorAll('[data-effect]').forEach(item => {
        item.addEventListener('mousedown', e => {
            e.preventDefault();
            const effect = item.dataset.effect;
            const sel = window.getSelection();
            if (sel && sel.rangeCount && !sel.isCollapsed) {
                const range = sel.getRangeAt(0);
                // Remove existing effect spans first
                const existing = range.commonAncestorContainer.parentElement?.closest('[data-text-effect]');
                if (existing) existing.replaceWith(...existing.childNodes);
                if (effect !== 'none') {
                    const span = document.createElement('span');
                    span.dataset.textEffect = effect;
                    span.classList.add(`effect-${effect}`);
                    try { range.surroundContents(span); } catch(_) {}
                }
            }
            picker.hidden = true;
            editor.focus();
            editor.dispatchEvent(new Event('input'));
            pushUndoSnapshot();
        });
    });

    document.addEventListener('click', e => {
        if (!picker.hidden && !picker.contains(e.target) && e.target !== btn) picker.hidden = true;
    });
})();

// ── Show / Hide ¶ (H50) ───────────────────────────────────────────────────────
(function() {
    const btn = document.getElementById('btn-show-marks');
    if (!btn) return;
    let marksOn = false;
    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        marksOn = !marksOn;
        editor.classList.toggle('show-marks', marksOn);
        btn.classList.toggle('rbtn--active-toggle', marksOn);
    });
    // Ctrl+Shift+8 shortcut
    document.addEventListener('keydown', e => {
        if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === '8') {
            e.preventDefault();
            marksOn = !marksOn;
            editor.classList.toggle('show-marks', marksOn);
            btn?.classList.toggle('rbtn--active-toggle', marksOn);
        }
    });
})();

// ── Paragraph Shading picker (H52) ────────────────────────────────────────────
(function() {
    const btn    = document.getElementById('btn-shading');
    const picker = document.getElementById('shading-picker');
    if (!btn || !picker) return;
    let _savedRange = null;

    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const sel = window.getSelection();
        if (sel && sel.rangeCount) _savedRange = sel.getRangeAt(0).cloneRange();
        picker.hidden = !picker.hidden;
        if (!picker.hidden) {
            const rect = btn.getBoundingClientRect();
            picker.style.position = 'fixed';
            picker.style.left = rect.left + 'px';
            picker.style.top  = (rect.bottom + 2) + 'px';
            picker.style.zIndex = '9999';
        }
    });

    function applyShading(color) {
        editor.focus();
        if (_savedRange) {
            const sel = window.getSelection();
            sel.removeAllRanges();
            sel.addRange(_savedRange);
        }
        const sel = window.getSelection();
        const anchor = sel?.anchorNode;
        const block = anchor ? anchor.parentElement?.closest('p,h1,h2,h3,h4,h5,h6,li,blockquote,pre,div') : null;
        if (block && editor.contains(block)) {
            block.style.backgroundColor = color === 'transparent' ? '' : color;
        }
        picker.hidden = true;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    }

    picker.querySelector('[data-shading="transparent"]')?.addEventListener('mousedown', e => {
        e.preventDefault(); applyShading('transparent');
    });
    picker.querySelectorAll('.color-swatch').forEach(sw => {
        sw.addEventListener('mousedown', e => { e.preventDefault(); applyShading(sw.dataset.shading); });
    });

    document.addEventListener('click', e => {
        if (!picker.hidden && !picker.contains(e.target) && e.target !== btn) picker.hidden = true;
    });
})();

// ── Borders quick-access picker (H53) ────────────────────────────────────────
(function() {
    const btn    = document.getElementById('btn-borders');
    const picker = document.getElementById('borders-picker');
    if (!btn || !picker) return;
    let _savedRange = null;

    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const sel = window.getSelection();
        if (sel && sel.rangeCount) _savedRange = sel.getRangeAt(0).cloneRange();
        picker.hidden = !picker.hidden;
        if (!picker.hidden) {
            const rect = btn.getBoundingClientRect();
            picker.style.position = 'fixed';
            picker.style.left = rect.left + 'px';
            picker.style.top  = (rect.bottom + 2) + 'px';
            picker.style.zIndex = '9999';
        }
    });

    const BORDER_STYLES = {
        none:    { borderTop: '', borderRight: '', borderBottom: '', borderLeft: '', border: 'none' },
        bottom:  { borderBottom: '1px solid currentColor' },
        top:     { borderTop: '1px solid currentColor' },
        left:    { borderLeft: '2px solid currentColor' },
        right:   { borderRight: '1px solid currentColor' },
        all:     { border: '1px solid currentColor' },
        outside: { border: '1px solid currentColor' },
    };

    picker.querySelectorAll('[data-border]').forEach(item => {
        item.addEventListener('mousedown', e => {
            e.preventDefault();
            editor.focus();
            if (_savedRange) {
                const sel = window.getSelection();
                sel.removeAllRanges();
                sel.addRange(_savedRange);
            }
            const sel = window.getSelection();
            const block = sel?.anchorNode?.parentElement?.closest('p,h1,h2,h3,h4,h5,h6,li,blockquote,pre,td,th');
            if (block && editor.contains(block)) {
                const styles = BORDER_STYLES[item.dataset.border] || {};
                // Clear existing borders first
                block.style.border = block.style.borderTop = block.style.borderRight =
                    block.style.borderBottom = block.style.borderLeft = '';
                Object.assign(block.style, styles);
            }
            picker.hidden = true;
            editor.dispatchEvent(new Event('input'));
            pushUndoSnapshot();
        });
    });

    document.addEventListener('click', e => {
        if (!picker.hidden && !picker.contains(e.target) && e.target !== btn) picker.hidden = true;
    });
})();

// ── Line & Paragraph Spacing picker (H01/H02) ────────────────────────────────
(function() {
    const btn    = document.getElementById('btn-spacing');
    const picker = document.getElementById('spacing-picker');
    if (!btn || !picker) return;
    let _savedRange = null;

    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const sel = window.getSelection();
        if (sel && sel.rangeCount) _savedRange = sel.getRangeAt(0).cloneRange();
        picker.hidden = !picker.hidden;
        if (!picker.hidden) {
            const rect = btn.getBoundingClientRect();
            picker.style.position = 'fixed';
            picker.style.left = rect.left + 'px';
            picker.style.top  = (rect.bottom + 2) + 'px';
            picker.style.zIndex = '9999';
        }
    });

    function getAnchorBlock(savedRange) {
        editor.focus();
        if (savedRange) {
            const sel = window.getSelection();
            sel.removeAllRanges();
            sel.addRange(savedRange);
        }
        const sel = window.getSelection();
        return sel?.anchorNode?.parentElement?.closest('p,h1,h2,h3,h4,h5,h6,li,blockquote,pre') || null;
    }

    picker.querySelectorAll('[data-spacing]').forEach(item => {
        item.addEventListener('mousedown', e => {
            e.preventDefault();
            const val = item.dataset.spacing;
            const block = getAnchorBlock(_savedRange);
            if (!block) { picker.hidden = true; return; }

            if (!isNaN(parseFloat(val))) {
                // Line height multiplier
                block.style.lineHeight = val;
            } else if (val === 'add-before') {
                const cur = parseFloat(block.style.marginBlockStart) || 0;
                block.style.marginBlockStart = (cur + 6) + 'pt';
            } else if (val === 'remove-before') {
                block.style.marginBlockStart = '0';
            } else if (val === 'add-after') {
                const cur = parseFloat(block.style.marginBlockEnd) || 0;
                block.style.marginBlockEnd = (cur + 6) + 'pt';
            } else if (val === 'remove-after') {
                block.style.marginBlockEnd = '0';
            }
            picker.hidden = true;
            editor.dispatchEvent(new Event('input'));
            pushUndoSnapshot();
        });
    });

    document.addEventListener('click', e => {
        if (!picker.hidden && !picker.contains(e.target) && e.target !== btn) picker.hidden = true;
    });
})();

// ── Sort (H51) ────────────────────────────────────────────────────────────────
(function() {
    const btn = document.getElementById('btn-sort');
    if (!btn) return;

    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const sel = window.getSelection();
        const anchor = sel?.anchorNode;

        // Case 1: cursor is inside a <ul> or <ol> — sort list items
        const list = anchor?.parentElement?.closest('ul,ol');
        if (list && editor.contains(list)) {
            const items = [...list.querySelectorAll(':scope > li')];
            items.sort((a, b) => a.textContent.trim().localeCompare(b.textContent.trim()));
            items.forEach(li => list.appendChild(li));
            editor.dispatchEvent(new Event('input'));
            pushUndoSnapshot();
            return;
        }

        // Case 2: cursor is inside a <table> — sort rows by first column of clicked row's tbody
        const table = anchor?.parentElement?.closest('table');
        if (table && editor.contains(table)) {
            const tbody = table.querySelector('tbody');
            if (!tbody) return;
            const rows = [...tbody.querySelectorAll('tr')];
            rows.sort((a, b) => {
                const ta = a.cells[0]?.textContent.trim() || '';
                const tb = b.cells[0]?.textContent.trim() || '';
                return ta.localeCompare(tb);
            });
            rows.forEach(r => tbody.appendChild(r));
            editor.dispatchEvent(new Event('input'));
            pushUndoSnapshot();
        }
    });
})();

// ── Editing group: Find / Replace / Select (H54, H55) ────────────────────────
(function() {
    // Find button in Home Editing group — surfaces existing Ctrl+F infrastructure
    document.getElementById('btn-find')?.addEventListener('click', () => openFind(false));
    // Replace button in Home Editing group — surfaces existing Ctrl+H infrastructure
    document.getElementById('btn-replace')?.addEventListener('click', () => openFind(true));

    // Select dropdown in Home Editing group
    const selectBtn    = document.getElementById('btn-select-menu');
    const selectPicker = document.getElementById('select-picker');
    if (selectBtn && selectPicker) {
        selectBtn.addEventListener('mousedown', e => {
            e.preventDefault();
            selectPicker.hidden = !selectPicker.hidden;
            if (!selectPicker.hidden) {
                const rect = selectBtn.getBoundingClientRect();
                selectPicker.style.position = 'fixed';
                selectPicker.style.left = rect.left + 'px';
                selectPicker.style.top  = (rect.bottom + 2) + 'px';
                selectPicker.style.zIndex = '9999';
            }
        });

        // "Select All Text with Similar Formatting" — H55
        const similarBtn = selectPicker.querySelector('[data-select="similar"]');
        if (similarBtn) {
            similarBtn.addEventListener('mousedown', e => {
                e.preventDefault();
                const sel = window.getSelection();
                if (!sel || !sel.anchorNode) return;
                const refEl = sel.anchorNode.parentElement;
                if (!refEl) return;
                const refCS = getComputedStyle(refEl);
                const refKey = `${refCS.fontFamily}|${refCS.fontSize}|${refCS.fontWeight}|${refCS.fontStyle}`;
                // Walk all inline elements and collect matching spans
                const range = document.createRange();
                let first = true;
                editor.querySelectorAll('span,[style]').forEach(el => {
                    const cs = getComputedStyle(el);
                    const key = `${cs.fontFamily}|${cs.fontSize}|${cs.fontWeight}|${cs.fontStyle}`;
                    if (key === refKey) {
                        if (first) { range.selectNodeContents(el); first = false; }
                    }
                });
                if (!first) { sel.removeAllRanges(); sel.addRange(range); }
                selectPicker.hidden = true;
            });
        }

        document.addEventListener('click', e => {
            if (!selectPicker.hidden && !selectPicker.contains(e.target) && e.target !== selectBtn)
                selectPicker.hidden = true;
        });
    }
})();

// ── Dictate (H56) ─────────────────────────────────────────────────────────────
(function() {
    const btn = document.getElementById('btn-dictate');
    if (!btn) return;

    const SR = window.SpeechRecognition || window.webkitSpeechRecognition;
    if (!SR) {
        btn.title = 'Dictate (not supported in this browser)';
        btn.disabled = true;
        return;
    }

    const recognition = new SR();
    recognition.continuous = true;
    recognition.interimResults = true;
    recognition.lang = navigator.language || 'en-US';

    let _dictating = false;
    let _interimNode = null;
    let _silenceTimer = null;

    recognition.onresult = (event) => {
        clearTimeout(_silenceTimer);
        let interim = '', final = '';
        for (let i = event.resultIndex; i < event.results.length; i++) {
            if (event.results[i].isFinal) final += event.results[i][0].transcript;
            else interim += event.results[i][0].transcript;
        }
        if (final) {
            if (_interimNode) { _interimNode.remove(); _interimNode = null; }
            document.execCommand('insertText', false, final + ' ');
            editor.dispatchEvent(new Event('input'));
        } else if (interim) {
            if (!_interimNode) {
                _interimNode = document.createElement('span');
                _interimNode.className = 'dictate-interim';
                _interimNode.style.cssText = 'color:var(--accent);opacity:0.6;';
                const sel = window.getSelection();
                if (sel && sel.rangeCount) sel.getRangeAt(0).insertNode(_interimNode);
            }
            _interimNode.textContent = interim;
        }
        // Stop after 2s of silence
        _silenceTimer = setTimeout(() => recognition.stop(), 2000);
    };

    recognition.onend = () => {
        _dictating = false;
        if (_interimNode) { _interimNode.remove(); _interimNode = null; }
        editor.style.cursor = '';
        btn.classList.remove('rbtn--active-toggle');
        btn.title = 'Dictate';
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    };

    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        if (_dictating) {
            recognition.stop();
        } else {
            editor.focus();
            recognition.start();
            _dictating = true;
            editor.style.cursor = 'text';
            btn.classList.add('rbtn--active-toggle');
            btn.title = 'Stop Dictating';
        }
    });
})();

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
// —— Grid Picker (L06: grows dynamically up to 12×12)
const TPICKER_MIN_ROWS = 8;
const TPICKER_MIN_COLS = 10;
const TPICKER_MAX = 12;
let tpickerMaxRows = TPICKER_MIN_ROWS;
let tpickerMaxCols = TPICKER_MIN_COLS;
const tablePicker  = document.getElementById('table-picker');
const tpickerGrid  = document.getElementById('tpicker-grid');
const tpickerLabel = document.getElementById('tpicker-label');

function rebuildGrid() {
    tpickerGrid.innerHTML = '';
    tpickerGrid.style.gridTemplateColumns = `repeat(${tpickerMaxCols}, 1fr)`;
    for (let r = 1; r <= tpickerMaxRows; r++) {
        for (let c = 1; c <= tpickerMaxCols; c++) {
            const cell = document.createElement('div');
            cell.className = 'tpicker-cell';
            cell.dataset.row = r;
            cell.dataset.col = c;
            tpickerGrid.appendChild(cell);
        }
    }
}
rebuildGrid();

function setPickerHighlight(rows, cols) {
    tpickerLabel.textContent = rows > 0 ? `${rows} × ${cols} Table` : 'Insert Table';
    [...tpickerGrid.children].forEach(cell => {
        const r = +cell.dataset.row, c = +cell.dataset.col;
        const active = r <= rows && c <= cols;
        cell.classList.toggle('tpicker-cell--hover', active);
    });
    // L06: Grow grid when cursor is within 1 cell of the edge
    let changed = false;
    if (rows >= tpickerMaxRows && tpickerMaxRows < TPICKER_MAX) { tpickerMaxRows = Math.min(tpickerMaxRows + 1, TPICKER_MAX); changed = true; }
    if (cols >= tpickerMaxCols && tpickerMaxCols < TPICKER_MAX) { tpickerMaxCols = Math.min(tpickerMaxCols + 1, TPICKER_MAX); changed = true; }
    if (changed) rebuildGrid();
}
// Saved selection — captured when the picker opens so focus leaving
// the editor does not prevent insertHTML from having an active range.
let _savedTableRange = null;

function openTablePicker() {
    // Capture current editor selection before focus leaves
    const sel = window.getSelection();
    if (sel && sel.rangeCount > 0) {
        _savedTableRange = sel.getRangeAt(0).cloneRange();
    } else {
        // No selection yet — place cursor at end of editor
        const range = document.createRange();
        range.selectNodeContents(editor);
        range.collapse(false);
        _savedTableRange = range;
    }
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
    // Restore the selection that existed when the picker was opened
    if (_savedTableRange) {
        editor.focus();
        const sel = window.getSelection();
        sel.removeAllRanges();
        sel.addRange(_savedTableRange);
        _savedTableRange = null;
    } else {
        editor.focus();
    }
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
    // Capture selection before prompt() steals focus
    const sel = window.getSelection();
    if (sel && sel.rangeCount > 0) _savedTableRange = sel.getRangeAt(0).cloneRange();
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

// ── Insert: Equation (M06) ───────────────────────────────────────────────────
const eqScrim = document.getElementById('equation-dialog-scrim');
const eqInput = document.getElementById('eq-latex-input');
const eqIsDisp = document.getElementById('eq-is-display');
const eqGrid = document.getElementById('eq-symbol-grid');

const EQ_SYMBOLS = [
    { s: 'α', t: '\\alpha' }, { s: 'β', t: '\\beta' }, { s: 'γ', t: '\\gamma' }, { s: 'θ', t: '\\theta' },
    { s: 'π', t: '\\pi' }, { s: 'σ', t: '\\sigma' }, { s: 'φ', t: '\\phi' }, { s: 'ω', t: '\\omega' },
    { s: 'Δ', t: '\\Delta' }, { s: 'Ω', t: '\\Omega' }, { s: 'Σ', t: '\\sum' }, { s: '∫', t: '\\int' },
    { s: '∂', t: '\\partial' }, { s: '∞', t: '\\infty' }, { s: '∇', t: '\\nabla' }, { s: '∏', t: '\\prod' },
    { s: '≤', t: '\\leq' }, { s: '≥', t: '\\geq' }, { s: '≠', t: '\\neq' }, { s: '≈', t: '\\approx' },
    { s: '×', t: '\\times' }, { s: '÷', t: '\\div' }, { s: '±', t: '\\pm' }, { s: '√', t: '\\sqrt{}' },
    { s: '→', t: '\\rightarrow' }, { s: 'x²', t: '^2' }, { s: 'xᵢ', t: '_i' }, { s: '½', t: '\\frac{1}{2}' }
];

if (eqGrid) {
    EQ_SYMBOLS.forEach(sym => {
        const btn = document.createElement('button');
        btn.className = 'symbol-btn';
        btn.textContent = sym.s;
        btn.title = sym.t;
        btn.style.cssText = 'padding:4px; font-size:12px; background:var(--chrome-bg); border:1px solid var(--border); border-radius:3px; cursor:pointer;';
        btn.addEventListener('click', () => {
            const pos = eqInput.selectionStart;
            const text = eqInput.value;
            eqInput.value = text.slice(0, pos) + sym.t + text.slice(pos);
            eqInput.focus();
            eqInput.selectionStart = eqInput.selectionEnd = pos + sym.t.length;
            if (sym.t.endsWith('{}')) eqInput.selectionStart--;
        });
        eqGrid.appendChild(btn);
    });
}

document.getElementById('btn-insert-equation').addEventListener('click', () => {
    if (!eqScrim) return;
    eqInput.value = '';
    eqScrim.hidden = false;
    eqInput.focus();
});

document.getElementById('btn-eq-cancel')?.addEventListener('click', () => {
    eqScrim.hidden = true;
});

document.getElementById('btn-eq-insert')?.addEventListener('click', () => {
    const math = eqInput.value.trim();
    eqScrim.hidden = true;
    if (!math) return;
    
    let html = '';
    if (eqIsDisp.checked) {
        html = `<div class="math-display">$$${math}$$</div><p></p>`;
    } else {
        html = `<span class="math-inline">$${math}$</span>`;
    }
    
    document.execCommand('insertHTML', false, html);
    
    flush().then(() => {
        invoke('md_to_html', { markdown: currentMarkdown }).then(res => {
            setEditorContent(res);
        });
    });
});

// ── Insert: Mermaid Diagram (M12) ──────────────────────────────────────────────────────────
const diagramScrim = document.getElementById('diagram-scrim');
const diagramSource = document.getElementById('diagram-source');
const diagramPreview = document.getElementById('diagram-preview');
const btnDiagramInsert = document.getElementById('btn-diagram-insert');
const btnDiagramCancel = document.getElementById('btn-diagram-cancel');

if (window.mermaid) {
    mermaid.initialize({ startOnLoad: false, theme: 'default' });
}

document.getElementById('btn-insert-diagram')?.addEventListener('click', () => {
    if (!diagramScrim) return;
    diagramScrim.hidden = false;
    diagramSource.focus();
    renderMermaidPreview();
});

btnDiagramCancel?.addEventListener('click', () => {
    diagramScrim.hidden = true;
});

async function renderMermaidPreview() {
    if (!window.mermaid) return;
    const code = diagramSource.value;
    diagramPreview.innerHTML = '';
    try {
        const { svg } = await mermaid.render('mermaid-preview-svg', code);
        diagramPreview.innerHTML = svg;
    } catch (e) {
        diagramPreview.innerHTML = `<div style="color:red; font-size:11px;">${e.message}</div>`;
    }
}

diagramSource?.addEventListener('input', () => {
    renderMermaidPreview();
});

btnDiagramInsert?.addEventListener('click', () => {
    const code = diagramSource.value.trim();
    if (!code) return;
    
    const svgContent = diagramPreview.innerHTML;
    const html = `<div class="mermaid-graph" contenteditable="false">${svgContent}</div><pre class="marksmen-roundtrip-meta" style="display:none">\`\`\`mermaid\n${code}\n\`\`\`</pre><p><br></p>`;
    
    diagramScrim.hidden = true;
    document.execCommand('insertHTML', false, html);
    editor.dispatchEvent(new Event('input'));
});

// ── Insert: Citation (Stage 2) ──────────────────────────────────────────────
const citationScrim = document.getElementById('citation-scrim');
const btnRefCite = document.getElementById('btn-ref-cite');
const btnCitationCancel = document.getElementById('btn-citation-cancel');
const btnCitationInsert = document.getElementById('btn-citation-insert');
const citationSearch = document.getElementById('citation-search');
const citationList = document.getElementById('citation-list');
let currentCitations = [];
let selectedCitation = null;

btnRefCite?.addEventListener('click', async () => {
    saveSelection();
    citationScrim.hidden = false;
    citationSearch.value = '';
    selectedCitation = null;
    btnCitationInsert.disabled = true;
    
    try {
        const dbStr = await window.__TAURI_INVOKE__('load_marksmen_cite_db');
        currentCitations = JSON.parse(dbStr);
        renderCitationList(currentCitations);
    } catch (e) {
        citationList.innerHTML = `<div style="color:red; font-size:12px;">Error loading citations: ${e}</div>`;
    }
});

btnCitationCancel?.addEventListener('click', () => {
    citationScrim.hidden = true;
    restoreSelection();
});

citationSearch?.addEventListener('input', () => {
    const query = citationSearch.value.toLowerCase();
    const filtered = currentCitations.filter(c => {
        const title = (c.title || '').toLowerCase();
        const authors = (c.authors || []).join(' ').toLowerCase();
        const year = (c.year || '').toLowerCase();
        return title.includes(query) || authors.includes(query) || year.includes(query);
    });
    renderCitationList(filtered);
});

function renderCitationList(citations) {
    if (!citations || citations.length === 0) {
        citationList.innerHTML = '<div style="font-size:12px; color:var(--text-secondary); text-align:center; padding: 20px;">No references found. Add some using marksmen-cite.</div>';
        return;
    }
    
    citationList.innerHTML = '';
    citations.forEach(c => {
        const el = document.createElement('div');
        el.className = 'citation-item';
        
        const title = document.createElement('div');
        title.className = 'citation-title';
        title.textContent = c.title || 'Untitled';
        
        const meta = document.createElement('div');
        meta.className = 'citation-meta';
        const authorsStr = (c.authors && c.authors.length > 0) ? c.authors.join(', ') : 'Unknown Author';
        meta.textContent = `${authorsStr} (${c.year || 'n.d.'})`;
        
        el.appendChild(title);
        el.appendChild(meta);
        
        el.addEventListener('click', () => {
            document.querySelectorAll('.citation-item').forEach(i => i.classList.remove('selected'));
            el.classList.add('selected');
            selectedCitation = c;
            btnCitationInsert.disabled = false;
        });
        
        citationList.appendChild(el);
    });
}

btnCitationInsert?.addEventListener('click', () => {
    if (!selectedCitation) return;
    citationScrim.hidden = true;
    restoreSelection();
    
    const citeId = selectedCitation.id || selectedCitation.doi || String(Date.now());
    const authorSurName = (selectedCitation.authors && selectedCitation.authors.length > 0) 
        ? selectedCitation.authors[0].split(' ').pop() 
        : 'Unknown';
    const display = `${authorSurName}, ${selectedCitation.year || 'n.d.'}`;
    
    const html = `<cite data-id="${citeId}" class="citation-node" contenteditable="false">[${display}]</cite>&nbsp;`;
    document.execCommand('insertHTML', false, html);
    editor.dispatchEvent(new Event('input'));
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

// ==========================================
// Phase 26: Track Changes (G-H40, G-H41) & Comment Resolution (G-H19)
// ==========================================

/**
 * Toggle Track Changes mode.
 * When active, every keystroke that inserts or deletes text is intercepted:
 *   - Inserted text is wrapped in <ins class="tc-insert" data-author data-date>.
 *   - Deleted text is replaced with <del class="tc-delete" data-author data-date>.
 */
function toggleTrackChanges() {
    isTrackChanges = !isTrackChanges;
    window.isTrackChanges = isTrackChanges; // expose for references.js
    document.body.classList.toggle('track-changes-active', isTrackChanges);
    const btn = document.getElementById('btn-track-changes');
    btn?.classList.toggle('rbtn--active', isTrackChanges);
    showToast(isTrackChanges ? 'Track Changes: ON' : 'Track Changes: OFF');
}

document.getElementById('btn-track-changes')?.addEventListener('click', toggleTrackChanges);

/**
 * Intercept keydown events in Track Changes mode.
 * Strategy:
 *   - Printable chars: prevent default, wrap new char in <ins>.
 *   - Backspace/Delete: prevent default, wrap the about-to-be-deleted char in <del>.
 */
editor.addEventListener('keydown', (e) => {
    if (!isTrackChanges || isDiffMode) return;

    const author = window.marksmenSettings?.author || 'You';
    const dateStr = new Date().toISOString();

    // Only handle printable single chars and Backspace/Delete
    const isPrintable = e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey;
    const isBackspace = e.key === 'Backspace';
    const isDelete    = e.key === 'Delete';

    if (!isPrintable && !isBackspace && !isDelete) return;

    const sel = window.getSelection();
    if (!sel.rangeCount) return;

    // Ensure selection is within the editor
    let node = sel.anchorNode;
    while (node && node !== editor) node = node.parentNode;
    if (!node) return;

    e.preventDefault();

    const range = sel.getRangeAt(0);

    if (!sel.isCollapsed) {
        // Selection: mark as deleted
        const fragment = range.extractContents();
        const del = document.createElement('del');
        del.className = 'tc-delete';
        del.dataset.author = author;
        del.dataset.date   = dateStr;
        // Accept / Reject inline buttons
        del.appendChild(_tcButtons(del));
        del.appendChild(fragment);
        range.insertNode(del);
        range.setStartAfter(del);
        range.collapse(true);
        sel.removeAllRanges();
        sel.addRange(range);

        if (isPrintable) {
            // Also insert the new character as tc-insert
            const ins = _tcInsertNode(e.key, author, dateStr);
            range.insertNode(ins);
            range.setStartAfter(ins);
            range.collapse(true);
            sel.removeAllRanges();
            sel.addRange(range);
        }
    } else if (isPrintable) {
        const ins = _tcInsertNode(e.key, author, dateStr);
        range.insertNode(ins);
        range.setStartAfter(ins);
        range.collapse(true);
        sel.removeAllRanges();
        sel.addRange(range);
    } else if (isBackspace) {
        // Delete char before cursor
        const tmpRange = range.cloneRange();
        tmpRange.modify('extend', 'backward', 'character');
        if (!tmpRange.collapsed) {
            const fragment = tmpRange.extractContents();
            const del = document.createElement('del');
            del.className = 'tc-delete';
            del.dataset.author = author;
            del.dataset.date   = dateStr;
            del.appendChild(_tcButtons(del));
            del.appendChild(fragment);
            tmpRange.insertNode(del);
            tmpRange.setStartAfter(del);
            tmpRange.collapse(true);
            sel.removeAllRanges();
            sel.addRange(tmpRange);
        }
    } else if (isDelete) {
        // Delete char after cursor
        const tmpRange = range.cloneRange();
        tmpRange.modify('extend', 'forward', 'character');
        if (!tmpRange.collapsed) {
            const fragment = tmpRange.extractContents();
            const del = document.createElement('del');
            del.className = 'tc-delete';
            del.dataset.author = author;
            del.dataset.date   = dateStr;
            del.appendChild(_tcButtons(del));
            del.appendChild(fragment);
            tmpRange.insertNode(del);
            tmpRange.setStartBefore(del);
            tmpRange.collapse(true);
            sel.removeAllRanges();
            sel.addRange(tmpRange);
        }
    }

    editor.dispatchEvent(new Event('input'));
    pushUndoSnapshot();
}, true); // capture phase so we intercept before contenteditable

/** Build an <ins> node for a single tracked insertion. */
function _tcInsertNode(char, author, dateStr) {
    const ins = document.createElement('ins');
    ins.className      = 'tc-insert';
    ins.dataset.author = author;
    ins.dataset.date   = dateStr;
    ins.textContent    = char;
    ins.appendChild(_tcButtons(ins));
    return ins;
}

/**
 * Build the inline Accept/Reject button pair for a tracked change node.
 * Returned as a DocumentFragment to append inside the ins/del element.
 */
function _tcButtons(changeNode) {
    const frag = document.createDocumentFragment();
    const acceptBtn = document.createElement('button');
    acceptBtn.className = 'tc-accept-btn';
    acceptBtn.title = 'Accept change';
    acceptBtn.textContent = '✔';
    acceptBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        _acceptChange(changeNode);
    });
    const rejectBtn = document.createElement('button');
    rejectBtn.className = 'tc-reject-btn';
    rejectBtn.title = 'Reject change';
    rejectBtn.textContent = '✘';
    rejectBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        _rejectChange(changeNode);
    });
    frag.appendChild(acceptBtn);
    frag.appendChild(rejectBtn);
    return frag;
}

/** Accept a single tracked change node. */
function _acceptChange(node) {
    const parent = node.parentNode;
    if (!parent) return;
    if (node.tagName === 'INS') {
        // Keep the text, remove the markup
        const text = node.textContent; // textContent excludes button text
        // Strip button text: only keep first text node(s)
        let textContent = '';
        node.childNodes.forEach(n => { if (n.nodeType === Node.TEXT_NODE) textContent += n.textContent; });
        parent.insertBefore(document.createTextNode(textContent), node);
    }
    // For DEL: accept deletion means removing the node entirely (text is gone)
    parent.removeChild(node);
    editor.dispatchEvent(new Event('input'));
    pushUndoSnapshot();
}

/** Reject a single tracked change node. */
function _rejectChange(node) {
    const parent = node.parentNode;
    if (!parent) return;
    if (node.tagName === 'DEL') {
        // Keep the deleted text, remove markup
        let textContent = '';
        node.childNodes.forEach(n => { if (n.nodeType === Node.TEXT_NODE) textContent += n.textContent; });
        parent.insertBefore(document.createTextNode(textContent), node);
    }
    // For INS: reject insertion means removing the inserted text entirely
    parent.removeChild(node);
    editor.dispatchEvent(new Event('input'));
    pushUndoSnapshot();
}

// Accept All Changes
document.getElementById('btn-accept-all')?.addEventListener('click', () => {
    const changes = [...editor.querySelectorAll('ins.tc-insert, del.tc-delete')];
    if (changes.length === 0) { showToast('No tracked changes found.'); return; }
    changes.forEach(node => _acceptChange(node));
    showToast(`Accepted ${changes.length} change(s).`);
});

// Reject All Changes
document.getElementById('btn-reject-all')?.addEventListener('click', () => {
    const changes = [...editor.querySelectorAll('ins.tc-insert, del.tc-delete')];
    if (changes.length === 0) { showToast('No tracked changes found.'); return; }
    changes.forEach(node => _rejectChange(node));
    showToast(`Rejected ${changes.length} change(s).`);
});

// Navigate Previous/Next Change
let _changeIdx = -1;
document.getElementById('btn-prev-change')?.addEventListener('click', () => {
    const changes = [...editor.querySelectorAll('ins.tc-insert, del.tc-delete')];
    if (changes.length === 0) { showToast('No tracked changes.'); return; }
    _changeIdx = (_changeIdx - 1 + changes.length) % changes.length;
    changes[_changeIdx].scrollIntoView({ behavior: 'smooth', block: 'center' });
    changes[_changeIdx].classList.add('active');
    setTimeout(() => changes[_changeIdx]?.classList.remove('active'), 1500);
});
document.getElementById('btn-next-change')?.addEventListener('click', () => {
    const changes = [...editor.querySelectorAll('ins.tc-insert, del.tc-delete')];
    if (changes.length === 0) { showToast('No tracked changes.'); return; }
    _changeIdx = (_changeIdx + 1) % changes.length;
    changes[_changeIdx].scrollIntoView({ behavior: 'smooth', block: 'center' });
    changes[_changeIdx].classList.add('active');
    setTimeout(() => changes[_changeIdx]?.classList.remove('active'), 1500);
});

// Resolve All Comments (G-H19)
document.getElementById('btn-resolve-all')?.addEventListener('click', () => {
    const marks = [...editor.querySelectorAll('mark.comment:not(.resolved)')];
    if (marks.length === 0) { showToast('No open comments.'); return; }
    marks.forEach(mark => {
        mark.classList.add('resolved');
        const id = mark.dataset.id;
        if (id && window.marksmenComments[id]) {
            window.marksmenComments[id].resolved = true;
        }
    });
    renderComments();
    showToast(`Resolved ${marks.length} comment(s).`);
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
        
        const isResolved = !!data.resolved;
        card.innerHTML = `
            <div style="display:flex; justify-content:space-between; align-items:center;">
                <div class="comment-author">${data.author}</div>
                <div style="display:flex; gap:4px; align-items:center;">
                    <button class="resolve-btn" style="background:none; border:1px solid var(--border); border-radius:3px; cursor:pointer; font-size:10px; padding:1px 6px; color:${isResolved ? 'var(--text-hint)' : 'var(--accent)'};" title="${isResolved ? 'Re-open comment' : 'Resolve comment'}">${isResolved ? '↺ Re-open' : '✔ Resolve'}</button>
                    <button class="delete-btn" style="background:none; border:none; cursor:pointer; font-size:12px; color:var(--text-hint);" title="Delete comment">✕</button>
                </div>
            </div>
            <div class="comment-context">"${data.context || '…'}"</div>
            <div class="comment-body">${data.body}</div>
            ${threadHtml}
            <div style="margin-top:8px; display:flex; gap:4px;">
                <input type="text" placeholder="Reply..." style="flex:1; border:1px solid var(--border); border-radius:3px; padding:2px 6px; font-size:11px; background:var(--chrome-bg); color:var(--text-primary);">
                <button class="reply-btn" style="background:var(--accent); color:white; border:none; border-radius:3px; padding:2px 8px; font-size:11px; cursor:pointer;">Reply</button>
            </div>
        `;
        if (isResolved) {
            card.classList.add('resolved');
            mark.classList.add('resolved');
        } else {
            mark.classList.remove('resolved');
        }
        
        // Wire events
        card.addEventListener('mouseenter', () => { mark.classList.add('active'); drawArrows(); });
        card.addEventListener('mouseleave', () => { mark.classList.remove('active'); drawArrows(); });
        mark.addEventListener('mouseenter', () => { card.classList.add('active'); card.scrollIntoView({behavior:'smooth', block:'nearest'}); drawArrows(); });
        mark.addEventListener('mouseleave', () => { card.classList.remove('active'); drawArrows(); });
        
        // Wire resolve/re-open button
        card.querySelector('.resolve-btn').addEventListener('click', () => {
            data.resolved = !data.resolved;
            mark.classList.toggle('resolved', data.resolved);
            editor.dispatchEvent(new Event('input'));
            renderComments();
        });

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
const btnToggleAi = document.getElementById('btn-toggle-ai');
if (btnToggleAi) {
    btnToggleAi.addEventListener('click', () => {
        sidebar.classList.remove('collapsed');
        const stabAi = document.getElementById('stab-ai');
        if (stabAi) stabAi.click();
        setTimeout(drawArrows, 200);
    });
}
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
    // C02: mark dirty and update title bar indicator
    if (!isDirty) {
        isDirty = true;
        document.title = '● ' + currentDocName + ' – Marksmen';
    }
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

async function doDiskAutosave() {
    if (currentMarkdown && currentDocName) {
        try {
            await invoke('autosave_file', { markdown: currentMarkdown, doc_name: currentDocName });
        } catch (e) {
            console.warn("Autosave failed", e);
        }
    }
}

async function flush() {
    if (isDiffMode) return;
    try {
        // [C03] Keep base64 images embedded in the markdown file
        // (Previously extracted to sidecar assets, now preserved per user request)

        // Embed comment metadata block
        const metadataHtml = `<script type="application/vnd.marksmen.comments">${JSON.stringify(window.marksmenComments)}</script>`;
        const payload = editor.innerHTML + '\n' + metadataHtml;
        currentMarkdown = await invoke('html_to_md', { html: payload });
        // C02: disk autosave also tracks clean state
        isDirty = false;
        document.title = currentDocName + ' – Marksmen';
        setStatus('● Saved');
        doDiskAutosave();
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
    const label = `${words.toLocaleString()} words \u00b7 ${chars.toLocaleString()} chars`;
    wordCount.textContent = label;
    // G-M30: also drive the status-bar word-count element (reset selection suffix)
    const sbarWc = document.getElementById('sbar-word-count');
    if (sbarWc) sbarWc.textContent = label;
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
    // F3 / Shift+F3 — navigate find matches when the bar is open (G-M24)
    if (e.key === 'F3' && !findBar.hidden) { e.preventDefault(); findStep(e.shiftKey ? -1 : 1); }
    // F3 without find bar open — open find bar
    if (e.key === 'F3' && findBar.hidden && !e.shiftKey) { e.preventDefault(); openFind(false); }
    if (ctrl && !e.shiftKey && e.key === 'z') { e.preventDefault(); performUndo(); }
    if (ctrl && e.shiftKey && e.key === 'Z') { e.preventDefault(); performRedo(); }
    if (ctrl && !e.shiftKey && e.key === 's') { e.preventDefault(); saveDocument(); }
    if (ctrl && e.shiftKey && e.key === 'S') { e.preventDefault(); saveDocumentAs(); }
    if (ctrl && e.shiftKey && e.key === 'V') { e.preventDefault(); pastePlain(); }
    if (e.key === 'Escape' && !findBar.hidden) { e.preventDefault(); closeFind(); }
    if (e.key === 'Escape') { closeCtxMenu(); }
    if (ctrl && e.key === 'p') { e.preventDefault(); printDocument(); }
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
function pushRecentFile(displayName, filePath) {
    let list = loadRecentFiles().filter(r => r.name !== displayName);
    list.unshift({ name: displayName, path: filePath || null });
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
    list.forEach(entry => {
        const { name, path } = typeof entry === 'string' ? { name: entry, path: null } : entry;
        const btn = document.createElement('button');
        btn.className = 'fmenu-recent-item';
        btn.innerHTML = `<span style="font-size:14px">&#128196;</span><span class="fmenu-recent-name">${name}</span>`;
        btn.title = path ? `Reopen ${path}` : `Reopen ${name}`;
        btn.addEventListener('click', async () => {
            fileMenu.hidden = true; fileScrim.hidden = true;
            if (!path) { document.getElementById('btn-open').click(); return; }
            setStatus('Opening…', 'syncing');
            try {
                const [md, filename, absolutePath] = await invoke('open_file_by_path', { path });
                currentMarkdown = md; baseMarkdown = md; currentFilePath = absolutePath;
                const lastDot = filename.lastIndexOf('.');
                const displayName2 = lastDot > 0 ? filename.slice(0, lastDot) : filename;
                currentDocName = displayName2;
                document.getElementById('doc-name').textContent = displayName2;
                document.title = displayName2 + ' – Marksmen';
                const html = await invoke('md_to_html', { markdown: md });
                setEditorContent(html);
                renderComments(); updateWordCount(); renderOutline(); updatePageCount();
                setStatus('● Saved');
            } catch(e) {
                setStatus('Error opening file', 'error'); console.error(e);
            }
        });
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
    if (e.detail?.name) pushRecentFile(e.detail.name, e.detail.path);
});

// ── Save / Save As ───────────────────────────────────────────────────────────
async function saveDocument() {
    await flush();
    try {
        setStatus('Saving…', 'syncing');
        const savedPath = await invoke('save_file', {
            markdown: currentMarkdown,
            current_path: currentFilePath ?? ''
        });
        currentFilePath = savedPath;
        isDirty = false;
        document.title = currentDocName + ' – Marksmen';
        setStatus('● Saved');
        pushRecentFile(currentDocName, savedPath);
    } catch(e) {
        if (e !== 'No file selected') { setStatus('Save error', 'error'); console.error(e); }
        else setStatus('● Saved');
    }
}
async function saveDocumentAs() {
    await flush();
    try {
        setStatus('Saving…', 'syncing');
        const savedPath = await invoke('save_file', { markdown: currentMarkdown, current_path: '' });
        currentFilePath = savedPath;
        isDirty = false;
        document.title = currentDocName + ' – Marksmen';
        setStatus('● Saved');
        pushRecentFile(currentDocName, savedPath);
    } catch(e) {
        if (e !== 'No file selected') { setStatus('Save error', 'error'); console.error(e); }
        else setStatus('● Saved');
    }
}
// C05: Expose isDirty for external eval and beforeunload
window.__isDirty = () => isDirty;
window.addEventListener('beforeunload', e => {
    if (isDirty) { e.preventDefault(); e.returnValue = 'You have unsaved changes. Close anyway?'; }
});
document.getElementById('btn-save')?.addEventListener('click', async () => {
    fileMenu.hidden = true; fileScrim.hidden = true;
    await saveDocument();
});
document.getElementById('btn-save-as')?.addEventListener('click', async () => {
    fileMenu.hidden = true; fileScrim.hidden = true;
    await saveDocumentAs();
});
document.getElementById('btn-file-exit')?.addEventListener('click', async () => {
    if (window.__TAURI__) {
        await window.__TAURI__.process.exit(0);
    } else {
        window.close();
    }
});

// ── Print via PDF ─────────────────────────────────────────────────────────────
async function printDocument() {
    await flush();
    try {
        setStatus('Printing…', 'syncing');
        await invoke('print_pdf', { markdown: currentMarkdown });
        setStatus('● Saved');
    } catch(e) {
        console.warn('print_pdf failed, falling back to window.print()', e);
        showToast('PDF print unavailable – check typst is in PATH. Using browser print.');
        window.print();
        setStatus('● Saved');
    }
}

// ── Paste Plain Text ─────────────────────────────────────────────────────────
async function pastePlain() {
    try {
        let text = '';
        if (window.__TAURI__ && window.__TAURI__.clipboard) {
            text = await window.__TAURI__.clipboard.readText();
        } else {
            text = await navigator.clipboard.readText();
        }
        document.execCommand('insertText', false, text);
    } catch(e) {
        console.warn('Clipboard read failed', e);
    }
}

// ── Toast notification ───────────────────────────────────────────────────────
function showToast(msg, durationMs = 4000) {
    let t = document.getElementById('marksmen-toast');
    if (!t) {
        t = document.createElement('div');
        t.id = 'marksmen-toast';
        t.style.cssText = 'position:fixed;bottom:60px;left:50%;transform:translateX(-50%);background:var(--text-primary);color:var(--page-bg);padding:8px 16px;border-radius:6px;font-size:12px;z-index:9999;pointer-events:none;opacity:0;transition:opacity 0.2s;';
        document.body.appendChild(t);
    }
    t.textContent = msg;
    t.style.opacity = '1';
    clearTimeout(t._timer);
    t._timer = setTimeout(() => { t.style.opacity = '0'; }, durationMs);
}
window.showToast = showToast; // expose for references.js

// ── Color Pickers ─────────────────────────────────────────────────────────────
const textColorPicker = document.getElementById('text-color-picker');
const highlightColorPicker = document.getElementById('highlight-color-picker');
textColorPicker?.addEventListener('input', e => {
    const color = e.target.value;
    document.documentElement.style.setProperty('--text-color-preview', color);
    document.execCommand('foreColor', false, color);
    editor.focus();
});
highlightColorPicker?.addEventListener('input', e => {
    const color = e.target.value;
    document.documentElement.style.setProperty('--highlight-color-preview', color);
    document.execCommand('hiliteColor', false, color);
    editor.focus();
});

// \u2500\u2500 Table Operations Engine \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500
// ── Table Operations Engine ──────────────────────────────────────────────────

function makeCell(tag = 'td', content = ' ') {
    const el = document.createElement(tag); el.textContent = content; return el;
}
function tableSync() { editor.dispatchEvent(new Event('input')); }

// Row operations
function addRowAbove(cell) {
    const row = cell.closest('tr'), newRow = document.createElement('tr');
    [...row.children].forEach(c => newRow.appendChild(makeCell(c.tagName.toLowerCase())));
    row.parentElement.insertBefore(newRow, row); tableSync();
}
function addRowBelow(cell) {
    const row = cell.closest('tr'), newRow = document.createElement('tr');
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

// Column operations
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
    } else { clearCellSelection(); cell.classList.add('cell-selected'); selectedCells.add(cell); }
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

// Alignment and table-wide select
function alignCells(align) {
    const targets = selectedCells.size > 0 ? [...selectedCells] : (ctxTargetCell ? [ctxTargetCell] : []);
    targets.forEach(c => { c.style.textAlign = align; }); tableSync();
}
function selectTable(cell) {
    const table = cell.closest('table');
    clearCellSelection();
    [...table.querySelectorAll('td, th')].forEach(c => { c.classList.add('cell-selected'); selectedCells.add(c); });
}

let ctxTargetParagraph = null;

// Context menu (right-click still works)
editor.addEventListener('contextmenu', e => {
    e.preventDefault();
    const cell = e.target.closest('td, th');
    
    // Hide table-specific options if not in table
    const isTable = !!cell;
    document.querySelectorAll('#ctx-menu .ctx-item:not(#ctx-lang-props)').forEach(li => {
        if (li.id !== 'ctx-lang-props') li.style.display = isTable ? '' : 'none';
    });
    document.querySelectorAll('#ctx-menu .ctx-sep:not(#ctx-lang-sep)').forEach(sep => {
        if (sep.id !== 'ctx-lang-sep') sep.style.display = isTable ? '' : 'none';
    });

    if (isTable) {
        ctxTargetCell = cell;
        if (!selectedCells.has(cell)) { clearCellSelection(); cell.classList.add('cell-selected'); selectedCells.add(cell); }
        document.getElementById('ctx-merge-cells').style.display = selectedCells.size >= 2 ? '' : 'none';
        document.getElementById('ctx-split-cell').style.display  = (cell.colSpan > 1 || cell.rowSpan > 1) ? '' : 'none';
    } else {
        ctxTargetCell = null;
    }

    // Set paragraph target
    let block = e.target;
    if (block.nodeType === 3) block = block.parentElement;
    const blockTags = ['P', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'LI', 'DIV', 'TD', 'TH'];
    while (block && block !== editor && !blockTags.includes(block.tagName)) {
        block = block.parentElement;
    }
    ctxTargetParagraph = (block && block !== editor) ? block : null;

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
ctxWire('ctx-cell-props',    () => showCellPropsDialog());

// ── Table Hover Bar ──────────────────────────────────────────────────────────
const tableHoverBar = document.getElementById('table-hover-bar');
let _hoverHideTimer = null;
let _hoverCell = null;  // cell the bar is currently anchored to

function showHoverBar(cell) {
    clearTimeout(_hoverHideTimer);
    _hoverCell = cell;

    // Recalculate position anchored to the row's bounding box
    const row  = cell.closest('tr');
    const rect = row.getBoundingClientRect();

    // Position: vertically centred on the row, to the left of it (or right if no space)
    const barH = tableHoverBar.offsetHeight || 32;
    let top  = rect.top + (rect.height - barH) / 2;
    let left = rect.left - tableHoverBar.offsetWidth - 8;
    if (left < 4) left = rect.right + 8; // flip to right if no left space

    tableHoverBar.style.top  = Math.max(4, top)  + 'px';
    tableHoverBar.style.left = Math.max(4, left) + 'px';
    tableHoverBar.hidden = false;
    // Force reflow so the CSS transition fires
    tableHoverBar.getBoundingClientRect();
    tableHoverBar.classList.add('thb-visible');

    // Keep ctxTargetCell in sync for operations
    ctxTargetCell = cell;
    // M22: track the active table for the Style picker
    window._ctxTable = cell.closest('table');
    // Update merge/split button visibility
    document.getElementById('thb-merge').style.display = selectedCells.size >= 2 ? '' : 'none';
    document.getElementById('thb-split').style.display =
        (cell.colSpan > 1 || cell.rowSpan > 1) ? '' : 'none';
}

function hideHoverBar(delay = 200) {
    clearTimeout(_hoverHideTimer);
    _hoverHideTimer = setTimeout(() => {
        tableHoverBar.classList.remove('thb-visible');
        // Wait for transition before hiding
        setTimeout(() => { tableHoverBar.hidden = true; _hoverCell = null; }, 150);
    }, delay);
}

// Trigger on cell mouseover inside the editor
editor.addEventListener('mouseover', e => {
    const cell = e.target.closest('td, th');
    if (!cell || !cell.closest('table')) { return; }
    showHoverBar(cell);
});

// Keep bar alive when mouse moves onto it
tableHoverBar.addEventListener('mouseenter', () => clearTimeout(_hoverHideTimer));
tableHoverBar.addEventListener('mouseleave', () => hideHoverBar(80));

// Hide when leaving the editor area
editor.addEventListener('mouseleave', () => hideHoverBar(300));

// Reposition bar on scroll/resize
document.addEventListener('scroll', () => { if (_hoverCell) showHoverBar(_hoverCell); }, true);

const thbWire = (id, fn) => document.getElementById(id)?.addEventListener('mousedown', e => {
    e.preventDefault(); // prevent blur
    fn();
    if (_hoverCell) showHoverBar(_hoverCell); // reposition after DOM change
});
thbWire('thb-add-row-above', () => _hoverCell && addRowAbove(_hoverCell));
thbWire('thb-add-row-below', () => _hoverCell && addRowBelow(_hoverCell));
thbWire('thb-add-col-left',  () => _hoverCell && addColLeft(_hoverCell));
thbWire('thb-add-col-right', () => _hoverCell && addColRight(_hoverCell));
thbWire('thb-merge',         () => mergeSelectedCells());
thbWire('thb-split',         () => _hoverCell && splitCell(_hoverCell));
thbWire('thb-align-left',    () => alignCells('left'));
thbWire('thb-align-center',  () => alignCells('center'));
thbWire('thb-align-right',   () => alignCells('right'));
thbWire('thb-del-row',       () => { if (_hoverCell) { deleteRow(_hoverCell); hideHoverBar(0); } });
thbWire('thb-del-col',       () => { if (_hoverCell) { deleteCol(_hoverCell); hideHoverBar(0); } });
thbWire('thb-del-table',     () => { if (_hoverCell) { deleteTable(_hoverCell); hideHoverBar(0); } });

// ── Phase 17: Typography & Paragraph Formatting ───────────────────────────────

// 1. Show/Hide Formatting Marks
let _showMarks = false;
document.getElementById('btn-show-marks')?.addEventListener('click', e => {
    e.preventDefault();
    _showMarks = !_showMarks;
    editor.classList.toggle('show-marks', _showMarks);
    e.currentTarget.classList.toggle('rbtn--active-toggle', _showMarks);
});

// 2. Change Case
const casePicker = document.getElementById('case-picker');
document.getElementById('btn-change-case')?.addEventListener('click', e => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    casePicker.style.top = (rect.bottom + 4) + 'px';
    casePicker.style.left = rect.left + 'px';
    casePicker.hidden = false;
});
document.addEventListener('mousedown', e => {
    if (casePicker && !casePicker.hidden && !e.target.closest('#case-picker') && !e.target.closest('#btn-change-case')) {
        casePicker.hidden = true;
    }
});
casePicker?.querySelectorAll('.fmenu-item').forEach(btn => {
    btn.addEventListener('click', e => {
        casePicker.hidden = true;
        const mode = e.currentTarget.dataset.case;
        const sel = window.getSelection();
        if (!sel || sel.isCollapsed || !editor.contains(sel.anchorNode)) return;
        
        const text = sel.toString();
        let newText = text;
        if (mode === 'lower') newText = text.toLowerCase();
        else if (mode === 'upper') newText = text.toUpperCase();
        else if (mode === 'title') newText = text.replace(/\b\w/g, c => c.toUpperCase());
        else if (mode === 'sentence') newText = text.charAt(0).toUpperCase() + text.slice(1).toLowerCase();
        else if (mode === 'toggle') newText = text.split('').map(c => c === c.toUpperCase() ? c.toLowerCase() : c.toUpperCase()).join('');
        
        document.execCommand('insertText', false, newText);
        editor.focus();
    });
});

// 3. Paragraph Spacing
const spacingPicker = document.getElementById('spacing-picker');
document.getElementById('btn-spacing')?.addEventListener('click', e => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    spacingPicker.style.top = (rect.bottom + 4) + 'px';
    spacingPicker.style.left = rect.left + 'px';
    spacingPicker.hidden = false;
});
document.addEventListener('mousedown', e => {
    if (spacingPicker && !spacingPicker.hidden && !e.target.closest('#spacing-picker') && !e.target.closest('#btn-spacing')) {
        spacingPicker.hidden = true;
    }
});
spacingPicker?.querySelectorAll('.fmenu-item').forEach(btn => {
    btn.addEventListener('click', e => {
        spacingPicker.hidden = true;
        const val = e.currentTarget.dataset.spacing;
        const sel = window.getSelection();
        if (!sel || !editor.contains(sel.anchorNode)) return;
        
        // Find selected blocks
        const range = sel.getRangeAt(0);
        const walker = document.createTreeWalker(editor, NodeFilter.SHOW_ELEMENT, {
            acceptNode: (node) => {
                if (['P', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'LI', 'BLOCKQUOTE'].includes(node.tagName)) {
                    if (range.intersectsNode(node)) return NodeFilter.FILTER_ACCEPT;
                }
                return NodeFilter.FILTER_SKIP;
            }
        });
        
        let node;
        let modified = false;
        disconnectObserver();
        while ((node = walker.nextNode())) {
            modified = true;
            if (val === 'add-before') node.style.marginTop = '12pt';
            else if (val === 'remove-before') node.style.marginTop = '0';
            else if (val === 'add-after') node.style.marginBottom = '12pt';
            else if (val === 'remove-after') node.style.marginBottom = '0';
            else node.style.lineHeight = val; // "1.0", "1.15", "1.5", "2.0"
        }
        reconnectObserver();
        
        if (modified) {
            pushUndoSnapshot();
            tableSync(); // dispatches 'input'
        }
        editor.focus();
    });
});

// 4. Shading & Borders
const shadingPicker = document.getElementById('shading-picker');
document.getElementById('btn-shading')?.addEventListener('click', e => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    shadingPicker.style.top = (rect.bottom + 4) + 'px';
    shadingPicker.style.left = rect.left + 'px';
    shadingPicker.hidden = false;
});
document.addEventListener('mousedown', e => {
    if (shadingPicker && !shadingPicker.hidden && !e.target.closest('#shading-picker') && !e.target.closest('#btn-shading')) {
        shadingPicker.hidden = true;
    }
});
shadingPicker?.querySelectorAll('.fmenu-item, .color-swatch').forEach(btn => {
    btn.addEventListener('click', e => {
        shadingPicker.hidden = true;
        const color = e.currentTarget.dataset.shading;
        const sel = window.getSelection();
        if (!sel || !editor.contains(sel.anchorNode)) return;
        
        const range = sel.getRangeAt(0);
        const walker = document.createTreeWalker(editor, NodeFilter.SHOW_ELEMENT, {
            acceptNode: (node) => {
                if (['P', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'LI', 'BLOCKQUOTE'].includes(node.tagName)) {
                    if (range.intersectsNode(node)) return NodeFilter.FILTER_ACCEPT;
                }
                return NodeFilter.FILTER_SKIP;
            }
        });
        
        let node;
        let modified = false;
        disconnectObserver();
        while ((node = walker.nextNode())) {
            modified = true;
            node.style.backgroundColor = color === 'transparent' ? '' : color;
        }
        reconnectObserver();
        
        if (modified) {
            pushUndoSnapshot();
            tableSync();
        }
        editor.focus();
    });
});

const bordersPicker = document.getElementById('borders-picker');
document.getElementById('btn-borders')?.addEventListener('click', e => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    bordersPicker.style.top = (rect.bottom + 4) + 'px';
    bordersPicker.style.left = rect.left + 'px';
    bordersPicker.hidden = false;
});
document.addEventListener('mousedown', e => {
    if (bordersPicker && !bordersPicker.hidden && !e.target.closest('#borders-picker') && !e.target.closest('#btn-borders')) {
        bordersPicker.hidden = true;
    }
});
bordersPicker?.querySelectorAll('.fmenu-item').forEach(btn => {
    btn.addEventListener('click', e => {
        bordersPicker.hidden = true;
        const mode = e.currentTarget.dataset.border;
        const sel = window.getSelection();
        if (!sel || !editor.contains(sel.anchorNode)) return;
        
        const range = sel.getRangeAt(0);
        const walker = document.createTreeWalker(editor, NodeFilter.SHOW_ELEMENT, {
            acceptNode: (node) => {
                if (['P', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'LI', 'BLOCKQUOTE'].includes(node.tagName)) {
                    if (range.intersectsNode(node)) return NodeFilter.FILTER_ACCEPT;
                }
                return NodeFilter.FILTER_SKIP;
            }
        });
        
        let node;
        let modified = false;
        disconnectObserver();
        while ((node = walker.nextNode())) {
            modified = true;
            node.style.border = '';
            node.style.borderTop = '';
            node.style.borderBottom = '';
            node.style.borderLeft = '';
            node.style.borderRight = '';
            const style = '1px solid var(--text-primary)';
            if (mode === 'all' || mode === 'outside') node.style.border = style;
            else if (mode === 'top') node.style.borderTop = style;
            else if (mode === 'bottom') node.style.borderBottom = style;
            else if (mode === 'left') node.style.borderLeft = style;
            else if (mode === 'right') node.style.borderRight = style;
        }
        reconnectObserver();
        
        if (modified) {
            pushUndoSnapshot();
            tableSync();
        }
        editor.focus();
    });
});

// 5. Font Grow/Shrink
const _fontSizes = [8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72];
function changeFontSize(step) {
    const sizePicker = document.getElementById('font-size-picker');
    if (!sizePicker) return;
    let currentVal = parseInt(document.queryCommandValue('fontSize')) || 3; 
    // Wait, queryCommandValue('fontSize') returns 1-7 in old execCommand.
    // However, if we applied specific points using `span style="font-size: 14pt"`, we'd need to read it.
    // But since marksmen-editor just uses the dropdown, let's read the dropdown's current value.
    let currentPt = parseInt(sizePicker.value);
    let idx = _fontSizes.indexOf(currentPt);
    if (idx === -1) idx = _fontSizes.findIndex(s => s >= currentPt);
    if (idx === -1) idx = 3; // Default 11pt
    
    let newIdx = Math.max(0, Math.min(_fontSizes.length - 1, idx + step));
    let newPt = _fontSizes[newIdx];
    sizePicker.value = newPt;
    document.execCommand('fontSize', false, "7"); // Dummy size to create span
    
    // Fix up the newly created font tags with correct point sizes
    const sel = window.getSelection();
    if (!sel || !editor.contains(sel.anchorNode)) return;
    document.querySelectorAll('font[size="7"]').forEach(f => {
        const span = document.createElement('span');
        span.style.fontSize = newPt + 'pt';
        span.innerHTML = f.innerHTML;
        f.replaceWith(span);
    });
    editor.focus();
    editor.dispatchEvent(new Event('input'));
}
document.getElementById('btn-font-grow')?.addEventListener('mousedown', e => { e.preventDefault(); changeFontSize(1); });
document.getElementById('btn-font-shrink')?.addEventListener('mousedown', e => { e.preventDefault(); changeFontSize(-1); });

// 6. Format Painter
let _formatPainterStyles = null;
// btnPainter is declared at the top of the file

function disarmFormatPainter() {
    _formatPainterStyles = null;
    editor.classList.remove('format-painter-armed');
    btnPainter?.classList.remove('rbtn--active-toggle');
    editor.removeEventListener('mouseup', applyFormatPainter);
}

function applyFormatPainter(e) {
    if (!_formatPainterStyles) return;
    const sel = window.getSelection();
    if (sel && !sel.isCollapsed && editor.contains(sel.anchorNode)) {
        // Apply the captured styles
        if (_formatPainterStyles.bold !== document.queryCommandState('bold')) document.execCommand('bold', false, null);
        if (_formatPainterStyles.italic !== document.queryCommandState('italic')) document.execCommand('italic', false, null);
        if (_formatPainterStyles.underline !== document.queryCommandState('underline')) document.execCommand('underline', false, null);
        if (_formatPainterStyles.strikeThrough !== document.queryCommandState('strikeThrough')) document.execCommand('strikeThrough', false, null);
        
        if (_formatPainterStyles.fontName) document.execCommand('fontName', false, _formatPainterStyles.fontName);
        if (_formatPainterStyles.foreColor) document.execCommand('foreColor', false, _formatPainterStyles.foreColor);
        if (_formatPainterStyles.hiliteColor) document.execCommand('hiliteColor', false, _formatPainterStyles.hiliteColor);
        
        // fontSize needs manual span fixup if we were copying it, but simple approach:
        document.execCommand('fontSize', false, _formatPainterStyles.fontSize);
        
        editor.focus();
        editor.dispatchEvent(new Event('input'));
    }
    disarmFormatPainter();
}

btnPainter?.addEventListener('click', e => {
    e.preventDefault();
    if (_formatPainterStyles) {
        disarmFormatPainter();
    } else {
        const sel = window.getSelection();
        if (!sel || !editor.contains(sel.anchorNode)) return;
        
        _formatPainterStyles = {
            bold: document.queryCommandState('bold'),
            italic: document.queryCommandState('italic'),
            underline: document.queryCommandState('underline'),
            strikeThrough: document.queryCommandState('strikeThrough'),
            fontName: document.queryCommandValue('fontName'),
            fontSize: document.queryCommandValue('fontSize'),
            foreColor: document.queryCommandValue('foreColor'),
            hiliteColor: document.queryCommandValue('hiliteColor')
        };
        editor.classList.add('format-painter-armed');
        btnPainter.classList.add('rbtn--active-toggle');
        
        // Wait for next mouse up to apply
        editor.addEventListener('mouseup', applyFormatPainter);
    }
});

// 7. Styles Gallery (Phase 19)
document.querySelectorAll('.style-card').forEach(card => {
    card.addEventListener('mousedown', e => {
        e.preventDefault();
        const cmd = card.dataset.cmd;
        const val = card.dataset.val;
        if (cmd && val) {
            document.execCommand(cmd, false, val);
            editor.focus();
            editor.dispatchEvent(new Event('input'));
        }
    });
});

// 8. Editing Group (Phase 19)
document.getElementById('btn-find')?.addEventListener('click', e => {
    e.preventDefault();
    openFind(false);
});
document.getElementById('btn-replace')?.addEventListener('click', e => {
    e.preventDefault();
    openFind(true);
});

const selectPicker = document.getElementById('select-picker');
document.getElementById('btn-select-menu')?.addEventListener('click', e => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    selectPicker.style.top = (rect.bottom + 4) + 'px';
    selectPicker.style.left = rect.left + 'px';
    selectPicker.hidden = false;
});
document.addEventListener('mousedown', e => {
    if (selectPicker && !selectPicker.hidden && !e.target.closest('#select-picker') && !e.target.closest('#btn-select-menu')) {
        selectPicker.hidden = true;
    }
});
selectPicker?.querySelectorAll('.fmenu-item').forEach(btn => {
    btn.addEventListener('click', () => {
        selectPicker.hidden = true;
    });
});

async function initSystemFonts() {
    try {
        const fonts = await invoke('get_system_fonts');
        const datalist = document.getElementById('font-family-list');
        if (datalist && fonts && fonts.length > 0) {
            // Preserve built-in options, append system fonts not already listed
            const existing = new Set([...datalist.options].map(o => o.value.toLowerCase()));
            fonts.forEach(f => {
                if (!existing.has(f.toLowerCase())) {
                    const opt = document.createElement('option');
                    opt.value = f;
                    datalist.appendChild(opt);
                }
            });
            // Default picker display to Inter/Segoe UI/Arial
            const picker = document.getElementById('font-family-picker');
            if (picker && !picker.value) {
                if (fonts.includes('Inter')) picker.value = 'Inter';
                else if (fonts.includes('Segoe UI')) picker.value = 'Segoe UI';
                else if (fonts.includes('Arial')) picker.value = 'Arial';
            }
        }
    } catch(e) {
        console.warn('Failed to load system fonts:', e);
    }
}
initSystemFonts();

// ── Initial render ────────────────────────────────────────────────────────────
loadSettings();
applySettings();
renderRecentFiles();
updateWordCount();
renderOutline();
flush();

// ── Phase 20 features ─────────────────────────────────────────────────────────

document.getElementById('btn-undo')?.addEventListener('mousedown', e => {
    e.preventDefault(); performUndo();
});
document.getElementById('btn-redo')?.addEventListener('mousedown', e => {
    e.preventDefault(); performRedo();
});

// ── Print button ─────────────────────────────────────────────────────────────
document.getElementById('btn-print')?.addEventListener('click', () => window.print());

// ── Focus Mode ───────────────────────────────────────────────────────────────
let _focusMode = false;
function toggleFocusMode() {
    _focusMode = !_focusMode;
    document.body.classList.toggle('focus-mode', _focusMode);
    const btn = document.getElementById('btn-focus-mode');
    if (btn) btn.classList.toggle('rbtn--active-toggle', _focusMode);
}
document.getElementById('btn-focus-mode')?.addEventListener('click', toggleFocusMode);
document.addEventListener('keydown', e => {
    if (e.key === 'F11') { e.preventDefault(); toggleFocusMode(); }
    if (e.key === 'Escape' && _focusMode) toggleFocusMode();
});

// ── Spell-check toggle ────────────────────────────────────────────────────────
let _spellcheck = true;
document.getElementById('btn-spellcheck')?.addEventListener('click', () => {
    _spellcheck = !_spellcheck;
    editor.spellcheck = _spellcheck;
    const btn = document.getElementById('btn-spellcheck');
    if (btn) btn.classList.toggle('rbtn--active-toggle', _spellcheck);
});

// ── Inline Link Dialog (replaces prompt()) ────────────────────────────────────
const linkDialog   = document.getElementById('link-dialog');
const linkTextInp  = document.getElementById('link-text');
const linkUrlInp   = document.getElementById('link-url');
let _linkSavedRange = null;

function openLinkDialog(prefillText = '', prefillUrl = '') {
    // Save current selection
    const sel = window.getSelection();
    _linkSavedRange = sel && sel.rangeCount > 0 ? sel.getRangeAt(0).cloneRange() : null;

    linkTextInp.value = prefillText;
    linkUrlInp.value  = prefillUrl;

    // Centre dialog on screen
    linkDialog.hidden = false;
    linkDialog.style.top  = '50%';
    linkDialog.style.left = '50%';
    linkDialog.style.transform = 'translate(-50%, -50%)';
    linkUrlInp.focus();
}

function closeLinkDialog() {
    linkDialog.hidden = true;
    _linkSavedRange   = null;
    editor.focus();
}

function commitLink() {
    const url  = linkUrlInp.value.trim();
    if (!url) { closeLinkDialog(); return; }
    const text = linkTextInp.value.trim() || url;

    // Restore the saved selection
    editor.focus();
    if (_linkSavedRange) {
        const sel = window.getSelection();
        sel.removeAllRanges();
        sel.addRange(_linkSavedRange);
    }

    // If there was a non-collapsed selection, wrap it; otherwise insert
    const sel2 = window.getSelection();
    if (sel2 && !sel2.isCollapsed) {
        document.execCommand('createLink', false, url);
    } else {
        document.execCommand('insertHTML', false,
            `<a href="${url}" target="_blank" rel="noopener noreferrer">${text}</a>`);
    }
    editor.dispatchEvent(new Event('input'));
    closeLinkDialog();
}

document.getElementById('link-ok')?.addEventListener('click', commitLink);
document.getElementById('link-cancel')?.addEventListener('click', closeLinkDialog);
linkDialog?.addEventListener('keydown', e => {
    if (e.key === 'Enter')  { e.preventDefault(); commitLink(); }
    if (e.key === 'Escape') closeLinkDialog();
});

// Replace old prompt-based link button
document.getElementById('btn-insert-link')?.removeEventListener('click', () => {});
document.getElementById('btn-insert-link')?.addEventListener('click', () => {
    const sel  = window.getSelection();
    const text = sel && !sel.isCollapsed ? sel.toString() : '';
    openLinkDialog(text, 'https://');
});

// ── Floating Selection Toolbar ────────────────────────────────────────────────
const selToolbar = document.getElementById('sel-toolbar');
let _selHideTimer = null;

function showSelToolbar() {
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || !editor.contains(sel.anchorNode)) {
        hideSelToolbar(); return;
    }
    clearTimeout(_selHideTimer);
    const range = sel.getRangeAt(0);
    const rect  = range.getBoundingClientRect();
    if (rect.width === 0) { hideSelToolbar(); return; }

    selToolbar.hidden = false;
    // Position above the selection
    const tbW = selToolbar.offsetWidth  || 240;
    const tbH = selToolbar.offsetHeight || 36;
    let top  = rect.top  - tbH - 8;
    let left = rect.left + (rect.width - tbW) / 2;
    if (top < 8) top = rect.bottom + 8;  // flip below if no room
    left = Math.max(8, Math.min(left, window.innerWidth - tbW - 8));

    selToolbar.style.top  = top  + 'px';
    selToolbar.style.left = left + 'px';
    selToolbar.getBoundingClientRect(); // force reflow
    selToolbar.classList.add('sel-visible');
}

function hideSelToolbar(delay = 0) {
    clearTimeout(_selHideTimer);
    _selHideTimer = setTimeout(() => {
        selToolbar.classList.remove('sel-visible');
        setTimeout(() => { selToolbar.hidden = true; }, 130);
    }, delay);
}

editor.addEventListener('mouseup', () => setTimeout(showSelToolbar, 60));
editor.addEventListener('keyup', e => {
    if (e.shiftKey) setTimeout(showSelToolbar, 60);
    else hideSelToolbar();
});
document.addEventListener('selectionchange', () => {
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed) hideSelToolbar(200);
});

// Wire sel-toolbar formatting buttons
selToolbar?.querySelectorAll('.sel-btn[data-cmd]').forEach(btn => {
    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        document.execCommand(btn.dataset.cmd, false, null);
        editor.dispatchEvent(new Event('input'));
    });
});
document.getElementById('sel-link')?.addEventListener('mousedown', e => {
    e.preventDefault();
    const sel  = window.getSelection();
    const text = sel && !sel.isCollapsed ? sel.toString() : '';
    hideSelToolbar();
    openLinkDialog(text, 'https://');
});
document.getElementById('sel-comment')?.addEventListener('mousedown', e => {
    e.preventDefault();
    hideSelToolbar();
    document.getElementById('btn-insert-comment')?.click();
});
document.getElementById('sel-clear')?.addEventListener('mousedown', e => {
    e.preventDefault();
    document.execCommand('removeFormat');
    editor.dispatchEvent(new Event('input'));
    hideSelToolbar();
});
selToolbar?.addEventListener('mouseenter', () => clearTimeout(_selHideTimer));
selToolbar?.addEventListener('mouseleave', () => hideSelToolbar(120));

// ── Markdown Heading Shortcuts (# prefix in contenteditable) ─────────────────
// On Space keydown: detect if line starts with #, ##, etc. and convert to heading
editor.addEventListener('keydown', e => {
    if (e.key !== ' ') return;
    const sel = window.getSelection();
    if (!sel || !sel.rangeCount) return;
    const range = sel.getRangeAt(0);
    // Only act at the very start of a text node
    const node = range.startContainer;
    if (node.nodeType !== Node.TEXT_NODE) return;
    const lineText = node.textContent.slice(0, range.startOffset);
    const match = lineText.match(/^(#{1,6})$/);
    if (!match) return;
    e.preventDefault();
    const level = match[1].length;
    // Clear the # characters
    node.textContent = node.textContent.slice(match[1].length);
    // Convert the containing block to heading
    document.execCommand('formatBlock', false, `h${level}`);
    // Flash the heading
    const block = sel.anchorNode?.parentElement?.closest('h1,h2,h3,h4,h5,h6');
    if (block) {
        block.classList.add('heading-flash');
        let timerId = null;
        const clearFlash = () => {
            block.classList.remove('heading-flash');
            if (timerId) clearTimeout(timerId);
            editor.removeEventListener('input', clearFlash);
            editor.removeEventListener('keydown', clearFlash);
        };
        timerId = setTimeout(clearFlash, 800);
        editor.addEventListener('input', clearFlash, { once: true });
        editor.addEventListener('keydown', clearFlash, { once: true });
    }
    editor.dispatchEvent(new Event('input'));
});

// ── Markdown Paste Detection ──────────────────────────────────────────────────
// If pasted content looks like Markdown (has # headings, **, __, -, etc.)
// convert via the backend rather than pasting raw text.
editor.addEventListener('paste', async e => {
    const md = e.clipboardData?.getData('text/plain') || '';
    // Heuristic: looks like Markdown if it has heading markers, lists, or bold
    const looksLikeMd = /^#{1,6} |^\*{1,2}[^*]|\*\*|^[-*] |^[0-9]+\. /m.test(md);
    if (!looksLikeMd) return;  // let native paste handle it

    e.preventDefault();
    try {
        const html = await invoke('md_to_html', { markdown: md });
        document.execCommand('insertHTML', false, html);
        editor.dispatchEvent(new Event('input'));
    } catch(_) {
        // Fallback: paste as plain text
        document.execCommand('insertText', false, md);
    }
});

// ── LocalStorage Persistence ──────────────────────────────────────────────────
const LS_KEY = 'marksmen-autosave';

// (showToast already defined above — reuses marksmen-toast element)

// Persist editor HTML to localStorage after every flush
// (ribbonThemeBtn and system theme watcher handled in Phase 20 block above)
const _origFlush = flush;
window.flush = async function() {
    await _origFlush();
    try {
        localStorage.setItem(LS_KEY, JSON.stringify({
            html:    editor.innerHTML,
            name:    currentDocName,
            md:      currentMarkdown,
            ts:      Date.now()
        }));
    } catch(_) {} // storage quota — silently ignore
};

// ── Phase 20: Page Setup (Layout Tab) ─────────────────────────────────────────
let currentOrientation = 'portrait';
let currentPageSize = { width: 816, height: 1056 }; // Letter default

function applyPageGeometry() {
    const isLand = currentOrientation === 'landscape';
    const w = isLand ? currentPageSize.height : currentPageSize.width;
    const h = isLand ? currentPageSize.width  : currentPageSize.height;
    editor.style.width = w + 'px';
    editor.style.minHeight = h + 'px';
    updatePageCount();
}

// 1. Margins
const marginsPicker = document.getElementById('margins-picker');
document.getElementById('btn-margins')?.addEventListener('click', e => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    marginsPicker.style.top = (rect.bottom + 4) + 'px';
    marginsPicker.style.left = rect.left + 'px';
    marginsPicker.hidden = false;
});
marginsPicker?.querySelectorAll('.fmenu-item').forEach(btn => {
    btn.addEventListener('click', e => {
        marginsPicker.hidden = true;
        const pt = e.currentTarget.dataset.margin;
        if (pt) {
            editor.style.padding = pt + 'px';
            editor.dispatchEvent(new Event('input'));
            pushUndoSnapshot();
        }
    });
});

// 2. Orientation
const orientationPicker = document.getElementById('orientation-picker');
document.getElementById('btn-orientation')?.addEventListener('click', e => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    orientationPicker.style.top = (rect.bottom + 4) + 'px';
    orientationPicker.style.left = rect.left + 'px';
    orientationPicker.hidden = false;
});
orientationPicker?.querySelectorAll('.fmenu-item').forEach(btn => {
    btn.addEventListener('click', e => {
        orientationPicker.hidden = true;
        const orient = e.currentTarget.dataset.orientation;
        if (orient && orient !== currentOrientation) {
            currentOrientation = orient;
            applyPageGeometry();
            editor.dispatchEvent(new Event('input'));
            pushUndoSnapshot();
        }
    });
});

// 3. Page Size
const sizePicker = document.getElementById('size-picker');
document.getElementById('btn-page-size')?.addEventListener('click', e => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    sizePicker.style.top = (rect.bottom + 4) + 'px';
    sizePicker.style.left = rect.left + 'px';
    sizePicker.hidden = false;
});
sizePicker?.querySelectorAll('.fmenu-item').forEach(btn => {
    btn.addEventListener('click', e => {
        sizePicker.hidden = true;
        const size = e.currentTarget.dataset.size;
        if (size === 'letter') currentPageSize = { width: 816, height: 1056 };
        else if (size === 'legal') currentPageSize = { width: 816, height: 1344 };
        else if (size === 'a4') currentPageSize = { width: 794, height: 1123 };
        else if (size === 'executive') currentPageSize = { width: 696, height: 1008 };
        applyPageGeometry();
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    });
});

// Hide pickers on outside click
document.addEventListener('mousedown', e => {
    if (marginsPicker && !marginsPicker.hidden && !e.target.closest('#margins-picker') && !e.target.closest('#btn-margins')) marginsPicker.hidden = true;
    if (orientationPicker && !orientationPicker.hidden && !e.target.closest('#orientation-picker') && !e.target.closest('#btn-orientation')) orientationPicker.hidden = true;
    if (sizePicker && !sizePicker.hidden && !e.target.closest('#size-picker') && !e.target.closest('#btn-page-size')) sizePicker.hidden = true;
});

// ── Phase 21: Advanced Layout Control (Columns, Breaks, etc.) ──────────────────

// 4. Columns
const columnsPicker = document.getElementById('columns-picker');
document.getElementById('btn-columns')?.addEventListener('click', e => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    columnsPicker.style.top = (rect.bottom + 4) + 'px';
    columnsPicker.style.left = rect.left + 'px';
    columnsPicker.hidden = false;
});
columnsPicker?.querySelectorAll('.fmenu-item').forEach(btn => {
    btn.addEventListener('click', e => {
        columnsPicker.hidden = true;
        const count = e.currentTarget.dataset.columns;
        if (count) {
            editor.style.columnCount = count;
            editor.style.columnGap = '0.5in';
            editor.dispatchEvent(new Event('input'));
            pushUndoSnapshot();
        }
    });
});

// 5. Breaks
const breaksPicker = document.getElementById('breaks-picker');
document.getElementById('btn-breaks')?.addEventListener('click', e => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    breaksPicker.style.top = (rect.bottom + 4) + 'px';
    breaksPicker.style.left = rect.left + 'px';
    breaksPicker.hidden = false;
});
breaksPicker?.querySelectorAll('.fmenu-item[data-break]').forEach(btn => {
    btn.addEventListener('click', e => {
        breaksPicker.hidden = true;
        const breakType = e.currentTarget.dataset.break;
        if (breakType === 'page') {
            document.execCommand('insertHTML', false, '<hr class="page-break-sentinel" contenteditable="false">');
        } else if (breakType === 'column') {
            document.execCommand('insertHTML', false, '<hr class="column-break-sentinel" contenteditable="false">');
        }
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    });
});

// 6. Line Numbers
const lineNumbersPicker = document.getElementById('line-numbers-picker');
document.getElementById('btn-line-numbers')?.addEventListener('click', e => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    lineNumbersPicker.style.top = (rect.bottom + 4) + 'px';
    lineNumbersPicker.style.left = rect.left + 'px';
    lineNumbersPicker.hidden = false;
});
lineNumbersPicker?.querySelectorAll('.fmenu-item').forEach(btn => {
    btn.addEventListener('click', e => {
        lineNumbersPicker.hidden = true;
        const lineno = e.currentTarget.dataset.lineno;
        if (lineno === 'continuous') {
            editor.classList.add('show-line-numbers');
        } else {
            editor.classList.remove('show-line-numbers');
        }
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    });
});

// 7. Hyphenation
const hyphenationPicker = document.getElementById('hyphenation-picker');
document.getElementById('btn-hyphenation')?.addEventListener('click', e => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    hyphenationPicker.style.top = (rect.bottom + 4) + 'px';
    hyphenationPicker.style.left = rect.left + 'px';
    hyphenationPicker.hidden = false;
});
hyphenationPicker?.querySelectorAll('.fmenu-item').forEach(btn => {
    btn.addEventListener('click', e => {
        hyphenationPicker.hidden = true;
        const hyphen = e.currentTarget.dataset.hyphen;
        if (hyphen === 'auto') {
            editor.style.hyphens = 'auto';
        } else {
            editor.style.hyphens = 'none';
        }
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    });
});

// Add Phase 21 & Phase 24 & Phase 25 pickers to the outside click handler
document.addEventListener('mousedown', e => {
    if (columnsPicker && !columnsPicker.hidden && !e.target.closest('#columns-picker') && !e.target.closest('#btn-columns')) columnsPicker.hidden = true;
    if (breaksPicker && !breaksPicker.hidden && !e.target.closest('#breaks-picker') && !e.target.closest('#btn-breaks')) breaksPicker.hidden = true;
    if (lineNumbersPicker && !lineNumbersPicker.hidden && !e.target.closest('#line-numbers-picker') && !e.target.closest('#btn-line-numbers')) lineNumbersPicker.hidden = true;
    if (hyphenationPicker && !hyphenationPicker.hidden && !e.target.closest('#hyphenation-picker') && !e.target.closest('#btn-hyphenation')) hyphenationPicker.hidden = true;
    
    // Phase 24 & 25
    const underlinePicker = document.getElementById('underline-picker');
    const effectsPicker = document.getElementById('effects-picker');
    const bulletPicker = document.getElementById('bullet-picker');
    const numPicker = document.getElementById('num-picker');
    const watermarkPicker = document.getElementById('watermark-picker');
    
    if (underlinePicker && !underlinePicker.hidden && !e.target.closest('#underline-picker') && !e.target.closest('#btn-underline-menu')) underlinePicker.hidden = true;
    if (effectsPicker && !effectsPicker.hidden && !e.target.closest('#effects-picker') && !e.target.closest('#btn-text-effects')) effectsPicker.hidden = true;
    if (bulletPicker && !bulletPicker.hidden && !e.target.closest('#bullet-picker') && !e.target.closest('#btn-bullet-menu')) bulletPicker.hidden = true;
    if (numPicker && !numPicker.hidden && !e.target.closest('#num-picker') && !e.target.closest('#btn-num-menu')) numPicker.hidden = true;
    if (watermarkPicker && !watermarkPicker.hidden && !e.target.closest('#watermark-picker') && !e.target.closest('#btn-watermark')) watermarkPicker.hidden = true;
});

// ── Phase 22: Interactive Image Engine ───────────────────────────────────────
let activeImage = null;
const imageResizer = document.getElementById('image-resizer');
const imageWrapToolbar = document.getElementById('image-wrap-toolbar');

function updateImageResizerBounds() {
    if (!activeImage || !imageResizer || imageResizer.hidden) return;
    const rect = activeImage.getBoundingClientRect();
    const scrollY = window.scrollY || document.documentElement.scrollTop;
    const scrollX = window.scrollX || document.documentElement.scrollLeft;
    imageResizer.style.top = (rect.top + scrollY) + 'px';
    imageResizer.style.left = (rect.left + scrollX) + 'px';
    imageResizer.style.width = rect.width + 'px';
    imageResizer.style.height = rect.height + 'px';
    
    // Position wrap toolbar below
    if (imageWrapToolbar && !imageWrapToolbar.hidden) {
        imageWrapToolbar.style.top = (rect.bottom + scrollY + 10) + 'px';
        imageWrapToolbar.style.left = (rect.left + scrollX) + 'px';
    }
}

// 1. Selection
editor.addEventListener('click', e => {
    if (e.target.tagName === 'IMG') {
        activeImage = e.target;
        imageResizer.hidden = false;
        imageResizer.classList.add('active');
        imageWrapToolbar.hidden = false;
        updateImageResizerBounds();
    }
});

// 2. Click outside to deselect
document.addEventListener('mousedown', e => {
    if (activeImage && !e.target.closest('#image-resizer') && !e.target.closest('#image-wrap-toolbar') && e.target !== activeImage) {
        activeImage = null;
        imageResizer.hidden = true;
        imageResizer.classList.remove('active');
        imageWrapToolbar.hidden = true;
    }
});

// 3. Keep pinned on scroll/resize
window.addEventListener('scroll', updateImageResizerBounds, { passive: true });
window.addEventListener('resize', updateImageResizerBounds, { passive: true });
if (window.ResizeObserver) {
    new ResizeObserver(updateImageResizerBounds).observe(editor);
}

// 4. Drag & Drop Resizing
let isDraggingImage = false;
let dragDir = null;
let dragStartParams = null; // { x, y, startW, startH }

imageResizer?.addEventListener('mousedown', e => {
    if (e.target.classList.contains('resize-handle')) {
        e.preventDefault();
        e.stopPropagation();
        isDraggingImage = true;
        dragDir = e.target.dataset.dir;
        const rect = activeImage.getBoundingClientRect();
        dragStartParams = {
            x: e.clientX,
            y: e.clientY,
            startW: rect.width,
            startH: rect.height,
            aspect: rect.width / rect.height
        };
    }
});

document.addEventListener('mousemove', e => {
    if (!isDraggingImage || !activeImage || !dragStartParams) return;
    e.preventDefault();
    const dx = e.clientX - dragStartParams.x;
    const dy = e.clientY - dragStartParams.y;
    
    let newW = dragStartParams.startW;
    let newH = dragStartParams.startH;
    
    // Simple proportion-preserving lock for corner drags
    const keepAspect = (dragDir === 'nw' || dragDir === 'ne' || dragDir === 'sw' || dragDir === 'se');
    
    if (dragDir.includes('e')) newW += dx;
    if (dragDir.includes('w')) newW -= dx;
    if (dragDir.includes('s')) newH += dy;
    if (dragDir.includes('n')) newH -= dy;
    
    if (newW < 20) newW = 20;
    if (newH < 20) newH = 20;
    
    if (keepAspect) {
        // Tie height to width to maintain aspect ratio
        newH = newW / dragStartParams.aspect;
    }
    
    activeImage.style.width = newW + 'px';
    activeImage.style.height = newH + 'px';
    updateImageResizerBounds();
});

document.addEventListener('mouseup', e => {
    if (isDraggingImage) {
        isDraggingImage = false;
        dragDir = null;
        dragStartParams = null;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    }
});

// 5. Text Wrap Toolbar
imageWrapToolbar?.querySelectorAll('.sel-btn').forEach(btn => {
    btn.addEventListener('click', e => {
        if (!activeImage) return;
        const mode = e.currentTarget.dataset.wrap;
        // Reset old styles
        activeImage.style.float = 'none';
        activeImage.style.display = 'inline';
        activeImage.style.margin = '0';
        
        if (mode === 'inline') {
            // Default
        } else if (mode === 'left') {
            activeImage.style.float = 'left';
            activeImage.style.margin = '0 1em 1em 0';
        } else if (mode === 'right') {
            activeImage.style.float = 'right';
            activeImage.style.margin = '0 0 1em 1em';
        } else if (mode === 'block') {
            activeImage.style.display = 'block';
            activeImage.style.margin = '1em auto';
        }
        updateImageResizerBounds();
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    });
});

// Phase 23: Image Captions
document.getElementById('btn-insert-caption')?.addEventListener('click', e => {
    if (!activeImage) return;
    
    // If it's already in a figure, don't double wrap
    if (activeImage.parentElement.tagName === 'FIGURE') return;

    const figures = editor.querySelectorAll('figure.image-figure').length + 1;
    const figure = document.createElement('figure');
    figure.className = 'image-figure';
    
    // Copy any float/margin from image to figure, and reset on image
    figure.style.float = activeImage.style.float;
    figure.style.display = activeImage.style.display;
    figure.style.margin = activeImage.style.margin;
    activeImage.style.float = 'none';
    activeImage.style.display = 'inline';
    activeImage.style.margin = '0';
    
    const figcaption = document.createElement('figcaption');
    figcaption.contentEditable = "true";
    figcaption.textContent = `Figure ${figures}: [Type caption here]`;
    
    activeImage.replaceWith(figure);
    figure.appendChild(activeImage);
    figure.appendChild(figcaption);
    
    updateImageResizerBounds();
    editor.dispatchEvent(new Event('input'));
    pushUndoSnapshot();
});

// ── Phase 23: Insert Tab Completion ──────────────────────────────────────────

// Table of Contents
document.getElementById('btn-insert-toc')?.addEventListener('click', () => {
    const headings = Array.from(editor.querySelectorAll('h1, h2, h3'));
    if (headings.length === 0) {
        showToast("No headings found to generate TOC.");
        return;
    }

    let tocHTML = '<div class="toc-container" contenteditable="false"><div class="toc-title">Table of Contents</div><ul>';
    let currentLevel = 1;

    headings.forEach((h, i) => {
        if (!h.id) h.id = `heading-${i}-${Date.now()}`;
        
        const level = parseInt(h.tagName[1]); // 'H1' -> 1
        
        if (level > currentLevel) {
            tocHTML += '<ul>'.repeat(level - currentLevel);
        } else if (level < currentLevel) {
            tocHTML += '</ul>'.repeat(currentLevel - level);
        }
        
        tocHTML += `<li><a href="#${h.id}">${h.textContent}</a></li>`;
        currentLevel = level;
    });

    if (currentLevel > 1) {
        tocHTML += '</ul>'.repeat(currentLevel - 1);
    }
    tocHTML += '</ul></div><p><br></p>';

    document.execCommand('insertHTML', false, tocHTML);
    editor.dispatchEvent(new Event('input'));
    pushUndoSnapshot();
});

// Symbol Picker
const symbolPicker = document.getElementById('symbol-picker');
const btnSymbol = document.getElementById('btn-insert-symbol');

btnSymbol?.addEventListener('click', e => {
    e.stopPropagation();
    symbolPicker.hidden = !symbolPicker.hidden;
    
    if (!symbolPicker.hidden) {
        const rect = btnSymbol.getBoundingClientRect();
        symbolPicker.style.top = (rect.bottom + 4) + 'px';
        symbolPicker.style.left = rect.left + 'px';
    }
});

symbolPicker?.addEventListener('click', e => {
    const btn = e.target.closest('.symbol-btn');
    if (btn) {
        e.stopPropagation();
        const char = btn.dataset.char;
        editor.focus();
        document.execCommand('insertText', false, char);
        symbolPicker.hidden = true;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    }
});

document.addEventListener('mousedown', e => {
    if (symbolPicker && !symbolPicker.hidden && !e.target.closest('#symbol-picker') && !e.target.closest('#btn-insert-symbol')) {
        symbolPicker.hidden = true;
    }
});

// ── Sprint 14: Productivity & Insert Completion (M13, M36, M24/M34) ─────────
// Shapes
const shapePicker = document.getElementById('shape-picker');
const btnShape = document.getElementById('btn-insert-shape');

btnShape?.addEventListener('click', e => {
    e.stopPropagation();
    shapePicker.hidden = !shapePicker.hidden;
    if (!shapePicker.hidden) {
        const rect = btnShape.getBoundingClientRect();
        shapePicker.style.top = (rect.bottom + 4) + 'px';
        shapePicker.style.left = rect.left + 'px';
    }
});

shapePicker?.addEventListener('click', e => {
    const btn = e.target.closest('.fmenu-item');
    if (btn) {
        e.stopPropagation();
        const type = btn.dataset.shape;
        editor.focus();
        
        let svgHtml = '';
        if (type === 'rect') {
            svgHtml = `<figure class="marksmen-shape" contenteditable="false" style="display:inline-block; margin:4px;"><svg width="100" height="100" viewBox="0 0 100 100"><rect width="100" height="100" fill="var(--theme-accent, #2563eb)" rx="4"/></svg></figure>&nbsp;`;
        } else if (type === 'circle') {
            svgHtml = `<figure class="marksmen-shape" contenteditable="false" style="display:inline-block; margin:4px;"><svg width="100" height="100" viewBox="0 0 100 100"><circle cx="50" cy="50" r="50" fill="var(--theme-accent, #2563eb)" /></svg></figure>&nbsp;`;
        } else if (type === 'line') {
            svgHtml = `<figure class="marksmen-shape" contenteditable="false" style="display:inline-block; margin:4px;"><svg width="200" height="20" viewBox="0 0 200 20"><line x1="0" y1="10" x2="200" y2="10" stroke="var(--theme-accent, #2563eb)" stroke-width="4"/></svg></figure>&nbsp;`;
        }
        
        document.execCommand('insertHTML', false, svgHtml);
        shapePicker.hidden = true;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    }
});

document.addEventListener('mousedown', e => {
    if (shapePicker && !shapePicker.hidden && !e.target.closest('#shape-picker') && !e.target.closest('#btn-insert-shape')) {
        shapePicker.hidden = true;
    }
});

// Form Fields
const formPicker = document.getElementById('form-picker');
const btnForm = document.getElementById('btn-insert-form');

btnForm?.addEventListener('click', e => {
    e.stopPropagation();
    formPicker.hidden = !formPicker.hidden;
    if (!formPicker.hidden) {
        const rect = btnForm.getBoundingClientRect();
        formPicker.style.top = (rect.bottom + 4) + 'px';
        formPicker.style.left = rect.left + 'px';
    }
});

formPicker?.addEventListener('click', e => {
    const btn = e.target.closest('.fmenu-item');
    if (btn) {
        e.stopPropagation();
        const type = btn.dataset.form;
        editor.focus();
        
        let formHtml = '';
        if (type === 'text') {
            formHtml = `<span class="marksmen-form-field" contenteditable="false"><input type="text" placeholder="Enter text" style="padding:2px 4px; border:1px solid var(--border); border-radius:3px; background:var(--bg); color:var(--text);"></span>&nbsp;`;
        } else if (type === 'checkbox') {
            formHtml = `<span class="marksmen-form-field" contenteditable="false"><input type="checkbox" style="vertical-align:middle; width:16px; height:16px; accent-color:var(--theme-accent, #2563eb);"></span>&nbsp;`;
        } else if (type === 'dropdown') {
            formHtml = `<span class="marksmen-form-field" contenteditable="false"><select style="padding:2px; border:1px solid var(--border); border-radius:3px; background:var(--bg); color:var(--text);"><option>Option 1</option><option>Option 2</option><option>Option 3</option></select></span>&nbsp;`;
        }
        
        document.execCommand('insertHTML', false, formHtml);
        formPicker.hidden = true;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    }
});

document.addEventListener('mousedown', e => {
    if (formPicker && !formPicker.hidden && !e.target.closest('#form-picker') && !e.target.closest('#btn-insert-form')) {
        formPicker.hidden = true;
    }
});

// Readability Stats (M24/M34)
const btnReviewStats = document.getElementById('btn-review-stats');
const statsScrim = document.getElementById('stats-scrim');
const btnStatsClose = document.getElementById('btn-stats-close');

function countSyllables(word) {
    word = word.toLowerCase();
    if(word.length <= 3) return 1;
    word = word.replace(/(?:[^laeiouy]es|ed|[^laeiouy]e)$/, '');
    word = word.replace(/^y/, '');
    const match = word.match(/[aeiouy]{1,2}/g);
    return match ? match.length : 1;
}

btnReviewStats?.addEventListener('click', () => {
    const text = editor.innerText || '';
    const words = text.match(/\b[-?a-zA-Z0-9_]+\b/g) || [];
    const numWords = words.length;
    
    // Split by . ! ?
    const sentences = text.split(/[.!?]+/).filter(s => s.trim().length > 0);
    let numSentences = sentences.length;
    if (numSentences === 0 && numWords > 0) numSentences = 1;
    
    let numSyllables = 0;
    for (let w of words) {
        numSyllables += countSyllables(w);
    }
    
    let readingEase = 0;
    let gradeLevel = 0;
    
    if (numWords > 0 && numSentences > 0) {
        // Flesch-Kincaid Reading Ease
        readingEase = 206.835 - 1.015 * (numWords / numSentences) - 84.6 * (numSyllables / numWords);
        // Flesch-Kincaid Grade Level
        gradeLevel = 0.39 * (numWords / numSentences) + 11.8 * (numSyllables / numWords) - 15.59;
    }
    
    const elWords = document.getElementById('stat-words');
    const elSentences = document.getElementById('stat-sentences');
    const elSyllables = document.getElementById('stat-syllables');
    const elEase = document.getElementById('stat-ease');
    const elGrade = document.getElementById('stat-grade');
    
    if(elWords) elWords.textContent = numWords;
    if(elSentences) elSentences.textContent = numSentences;
    if(elSyllables) elSyllables.textContent = numSyllables;
    if(elEase) elEase.textContent = Math.max(0, readingEase).toFixed(1);
    if(elGrade) elGrade.textContent = Math.max(0, gradeLevel).toFixed(1);
    
    if (statsScrim) statsScrim.hidden = false;
});

btnStatsClose?.addEventListener('click', () => {
    if (statsScrim) statsScrim.hidden = true;
});

// ── Phase 24: Advanced Home Tab Typography & Tooling ──────────────────────────

// Underline Picker
const btnUnderlineMenu = document.getElementById('btn-underline-menu');
const underlinePicker = document.getElementById('underline-picker');
btnUnderlineMenu?.addEventListener('click', e => {
    e.stopPropagation();
    underlinePicker.hidden = !underlinePicker.hidden;
    if (!underlinePicker.hidden) {
        const rect = btnUnderlineMenu.getBoundingClientRect();
        underlinePicker.style.top = (rect.bottom + 4) + 'px';
        underlinePicker.style.left = rect.left + 'px';
    }
});
underlinePicker?.addEventListener('click', e => {
    const btn = e.target.closest('.fmenu-item');
    if (btn) {
        e.stopPropagation();
        const style = btn.dataset.underline;
        const sel = window.getSelection();
        if (sel.rangeCount > 0 && !sel.isCollapsed) {
            const span = document.createElement('span');
            span.style.textDecoration = 'underline';
            span.style.textDecorationStyle = style;
            const range = sel.getRangeAt(0);
            span.appendChild(range.extractContents());
            range.insertNode(span);
            // Select the span contents
            sel.removeAllRanges();
            const newRange = document.createRange();
            newRange.selectNodeContents(span);
            sel.addRange(newRange);
        }
        underlinePicker.hidden = true;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    }
});

// Text Effects Picker
const btnEffectsMenu = document.getElementById('btn-text-effects');
const effectsPicker = document.getElementById('effects-picker');
btnEffectsMenu?.addEventListener('click', e => {
    e.stopPropagation();
    effectsPicker.hidden = !effectsPicker.hidden;
    if (!effectsPicker.hidden) {
        const rect = btnEffectsMenu.getBoundingClientRect();
        effectsPicker.style.top = (rect.bottom + 4) + 'px';
        effectsPicker.style.left = rect.left + 'px';
    }
});
effectsPicker?.addEventListener('click', e => {
    const btn = e.target.closest('.fmenu-item');
    if (btn) {
        e.stopPropagation();
        const effect = btn.dataset.effect;
        const sel = window.getSelection();
        if (sel.rangeCount > 0 && !sel.isCollapsed) {
            const span = document.createElement('span');
            if (effect !== 'none') {
                span.className = `effect-${effect}`;
            }
            const range = sel.getRangeAt(0);
            span.appendChild(range.extractContents());
            range.insertNode(span);
            sel.removeAllRanges();
            const newRange = document.createRange();
            newRange.selectNodeContents(span);
            sel.addRange(newRange);
        }
        effectsPicker.hidden = true;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    }
});

// Bullet Picker
const btnBulletMenu = document.getElementById('btn-bullet-menu');
const bulletPicker = document.getElementById('bullet-picker');
btnBulletMenu?.addEventListener('click', e => {
    e.stopPropagation();
    bulletPicker.hidden = !bulletPicker.hidden;
    if (!bulletPicker.hidden) {
        const rect = btnBulletMenu.getBoundingClientRect();
        bulletPicker.style.top = (rect.bottom + 4) + 'px';
        bulletPicker.style.left = rect.left + 'px';
    }
});
bulletPicker?.addEventListener('click', e => {
    const btn = e.target.closest('.fmenu-item');
    if (btn) {
        e.stopPropagation();
        const type = btn.dataset.list;
        editor.focus();
        document.execCommand('insertUnorderedList');
        const node = getSelectionNode();
        const ul = node?.closest('ul');
        if (ul) ul.style.listStyleType = type;
        bulletPicker.hidden = true;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    }
});

// Number Picker
const btnNumMenu = document.getElementById('btn-num-menu');
const numPicker = document.getElementById('num-picker');
btnNumMenu?.addEventListener('click', e => {
    e.stopPropagation();
    numPicker.hidden = !numPicker.hidden;
    if (!numPicker.hidden) {
        const rect = btnNumMenu.getBoundingClientRect();
        numPicker.style.top = (rect.bottom + 4) + 'px';
        numPicker.style.left = rect.left + 'px';
    }
});
numPicker?.addEventListener('click', e => {
    const btn = e.target.closest('.fmenu-item');
    if (btn) {
        e.stopPropagation();
        const type = btn.dataset.list;
        editor.focus();
        document.execCommand('insertOrderedList');
        const node = getSelectionNode();
        const ol = node?.closest('ol');
        if (ol) ol.style.listStyleType = type;
        numPicker.hidden = true;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    }
});

// Outline List Button
const btnOutlineList = document.getElementById('btn-outline-list');
btnOutlineList?.addEventListener('click', () => {
    editor.focus();
    document.execCommand('insertOrderedList');
    const node = getSelectionNode();
    const ol = node?.closest('ol');
    if (ol) {
        ol.classList.toggle('list-outline');
        if (ol.classList.contains('list-outline')) {
            ol.style.listStyleType = 'none';
        } else {
            ol.style.listStyleType = 'decimal';
        }
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    }
});

// Sort A-Z Button
const btnSort = document.getElementById('btn-sort');
btnSort?.addEventListener('click', () => {
    const node = getSelectionNode();
    if (!node) return;
    const list = node.closest('ul, ol');
    if (list) {
        const items = Array.from(list.children).filter(c => c.tagName === 'LI');
        items.sort((a, b) => a.textContent.trim().localeCompare(b.textContent.trim()));
        items.forEach(li => list.appendChild(li));
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    } else {
        showToast("Sorting only supported within lists.");
    }
});

// ==========================================
// Phase 25: Header, Footer, Watermark, Color
// ==========================================

const btnHeader = document.getElementById('btn-insert-header');
const btnFooter = document.getElementById('btn-insert-footer');
const btnPageNum = document.getElementById('btn-insert-page-num');
const btnWatermark = document.getElementById('btn-watermark');
const watermarkPicker = document.getElementById('watermark-picker');
const pageColorPicker = document.getElementById('page-color-picker');

function toggleHeaderFooter(type) {
    const className = type === 'header' ? 'doc-header' : 'doc-footer';
    let block = editor.querySelector(`.${className}`);
    if (block) {
        block.remove();
    } else {
        block = document.createElement('div');
        block.className = className;
        block.contentEditable = 'true';
        if (type === 'header') {
            editor.insertBefore(block, editor.firstChild);
        } else {
            editor.appendChild(block);
        }
        block.focus();
    }
    editor.dispatchEvent(new Event('input'));
    pushUndoSnapshot();
}

btnHeader?.addEventListener('click', () => toggleHeaderFooter('header'));
btnFooter?.addEventListener('click', () => toggleHeaderFooter('footer'));

btnPageNum?.addEventListener('click', () => {
    const sel = window.getSelection();
    if (!sel.rangeCount) return;
    const node = getSelectionNode();
    if (node && (node.closest('.doc-header') || node.closest('.doc-footer'))) {
        const token = document.createElement('span');
        token.className = 'page-number-token';
        token.contentEditable = 'false';
        const range = sel.getRangeAt(0);
        range.insertNode(token);
        range.setStartAfter(token);
        range.setEndAfter(token);
        sel.removeAllRanges();
        sel.addRange(range);
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    } else {
        showToast("Page Numbers must be inserted into a Header or Footer.");
    }
});

btnWatermark?.addEventListener('click', (e) => {
    e.stopPropagation();
    watermarkPicker.hidden = !watermarkPicker.hidden;
    if (!watermarkPicker.hidden) {
        const rect = btnWatermark.getBoundingClientRect();
        watermarkPicker.style.top = (rect.bottom + 4) + 'px';
        watermarkPicker.style.left = rect.left + 'px';
    }
});

watermarkPicker?.addEventListener('click', (e) => {
    const btn = e.target.closest('.fmenu-item');
    if (btn) {
        e.stopPropagation();
        const watermarkText = btn.dataset.watermark;
        let overlay = editorContainer.querySelector('.watermark-overlay');
        
        if (watermarkText === 'none') {
            if (overlay) overlay.remove();
        } else {
            if (!overlay) {
                overlay = document.createElement('div');
                overlay.className = 'watermark-overlay';
                editorContainer.appendChild(overlay);
            }
            overlay.textContent = watermarkText;
        }
        
        watermarkPicker.hidden = true;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    }
});

pageColorPicker?.addEventListener('input', (e) => {
    // Note: We use the editor variable which is #editor element
    editor.style.backgroundColor = e.target.value;
    editor.dispatchEvent(new Event('input'));
    pushUndoSnapshot();
});

(async function restoreFromAutosave() {
    try {
        // First try to load from disk via Tauri command
        const diskAutosave = await invoke('load_latest_autosave').catch(() => null);
        if (diskAutosave) {
            const [md, name] = diskAutosave;
            currentMarkdown = md;
            currentDocName  = name;
            document.getElementById('doc-name').textContent = name;
            document.title = name + ' – Marksmen';
            const html = await invoke('md_to_html', { markdown: md });
            setEditorContent(html);
            showToast(`↩ Restored disk autosave "${name}"`);
            return;
        }

        // Fallback to localStorage if disk autosave is missing
        const saved = localStorage.getItem(LS_KEY);
        if (!saved) return;
        const data = JSON.parse(saved);
        if (!data.html || Date.now() - data.ts > 30 * 86400_000) return;
        if (data.name && data.name !== 'Untitled Document') {
            setEditorContent(data.html);
            currentMarkdown = data.md || '';
            currentDocName  = data.name;
            document.getElementById('doc-name').textContent = data.name;
            document.title = data.name + ' – Marksmen';
            const age = Math.round((Date.now() - data.ts) / 60000);
            showToast(`↩ Restored "${data.name}" from ${age < 2 ? 'just now' : age + ' min ago'}`);
        }
    } catch(e) {
        console.error('Failed to restore autosave', e);
    }
})();

// ── Theme Toggle → sync with settings panel ───────────────────────────────────
const ribbonThemeBtn = document.getElementById('btn-toggle-theme');
if (ribbonThemeBtn) {
    ribbonThemeBtn.addEventListener('click', () => {
        const isDark = document.body.classList.toggle('dark-theme');
        settings.theme = isDark ? 'dark' : 'light';
        saveSettings();
        const sel = document.getElementById('s-theme');
        if (sel) sel.value = settings.theme;
    });
}

// ── System theme watcher ──────────────────────────────────────────────────────
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', e => {
    if (settings.theme === 'system') {
        document.body.classList.toggle('dark-theme', e.matches);
    }
});

// ── Bottom Status Bar ────────────────────────────────────────────────────────
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
// (btn-insert-link handler defined in Phase 20 using openLinkDialog())

// (ribbonThemeBtn + system-theme watcher handled in Phase 20)

// ── State Observer for Undo Stack ───────────────────────────────────────────
window.stateObserver = new MutationObserver((mutations) => {
    let meaningful = false;
    for (const m of mutations) {
        if (m.target === editor || (m.target && m.target.nodeType === 1 && editor.contains(m.target))) {
            meaningful = true;
            break;
        }
    }
    if (meaningful && !isUndoing && !isDiffMode) {
        clearTimeout(window.snapshotTimer);
        window.snapshotTimer = setTimeout(() => {
            snapshotState();
            isDirty = true;
        }, 500);
    }
});
window.stateObserver.observe(editor, { childList: true, characterData: true, subtree: true });

// ── Window Close Warning ────────────────────────────────────────────────────
let isClosing = false;
if (window.__TAURI__ && window.__TAURI__.window) {
    const { getCurrentWindow } = window.__TAURI__.window;
    const { ask } = window.__TAURI__.dialog;
    if (getCurrentWindow && ask) {
        getCurrentWindow().onCloseRequested(async (event) => {
            if (isClosing) return; // Prevent loop
            
            if (isDirty) {
                event.preventDefault();
                try {
                    const confirmed = await ask('You have unsaved changes. Are you sure you want to close without saving?', { 
                        title: 'Marksmen', 
                        kind: 'warning' 
                    });
                    if (confirmed) {
                        isDirty = false;
                        isClosing = true;
                        await getCurrentWindow().destroy();
                    }
                } catch(e) {
                    // Fallback force-close if dialog API fails
                    isClosing = true;
                    await getCurrentWindow().destroy();
                }
            }
        });
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Sprint 2 — Layout Panel Handlers (H03/H04/H05/H06/H10/H12/H31)
// ════════════════════════════════════════════════════════════════════════════

// ── Helper: set CSS variable on :root ────────────────────────────────────────
function setRootVar(name, value) {
    document.documentElement.style.setProperty(name, value);
}

// ── Page Setup picker opener factory ─────────────────────────────────────────
function makePickerToggle(btnId, pickerId) {
    const btn = document.getElementById(btnId);
    const picker = document.getElementById(pickerId);
    if (!btn || !picker) return;
    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const allPickers = document.querySelectorAll('.table-picker');
        allPickers.forEach(p => { if (p !== picker) p.hidden = true; });
        picker.hidden = !picker.hidden;
        if (!picker.hidden) {
            const rect = btn.getBoundingClientRect();
            picker.style.position = 'fixed';
            picker.style.left     = rect.left + 'px';
            picker.style.top      = (rect.bottom + 2) + 'px';
            picker.style.zIndex   = '9999';
        }
    });
    document.addEventListener('click', e => {
        if (!picker.hidden && !picker.contains(e.target) && e.target !== btn) picker.hidden = true;
    });
}

makePickerToggle('btn-margins',      'margins-picker');
makePickerToggle('btn-orientation',  'orientation-picker');
makePickerToggle('btn-page-size',    'size-picker');
makePickerToggle('btn-columns',      'columns-picker');
makePickerToggle('btn-breaks',       'breaks-picker');
makePickerToggle('btn-line-numbers', 'line-numbers-picker');
makePickerToggle('btn-hyphenation',  'hyphenation-picker');
makePickerToggle('btn-watermark',    'watermark-picker');

// ── H03: Margins ──────────────────────────────────────────────────────────────
(function() {
    // Preset buttons
    document.querySelectorAll('#margins-picker [data-margin]').forEach(btn => {
        btn.addEventListener('mousedown', e => {
            e.preventDefault();
            const px = btn.dataset.margin + 'px';
            setRootVar('--page-margin-top',    px);
            setRootVar('--page-margin-right',  px);
            setRootVar('--page-margin-bottom', px);
            setRootVar('--page-margin-left',   px);
            document.getElementById('margins-picker').hidden = true;
            pushUndoSnapshot();
        });
    });

    // Custom Margins dialog
    function openMarginsDialog() {
        const overlay = document.createElement('div');
        overlay.className = 'margins-dialog-overlay';
        const style = getComputedStyle(document.documentElement);
        const get = v => Math.round(parseFloat(style.getPropertyValue(v)) || 88);
        overlay.innerHTML = `
            <div class="margins-dialog">
                <h3>Custom Margins</h3>
                <div class="margins-dialog-grid">
                    <div><label>Top (px)<br><input type="number" id="md-top"    value="${get('--page-margin-top')}"    min="0" max="500" step="4"></label></div>
                    <div><label>Bottom (px)<br><input type="number" id="md-bot" value="${get('--page-margin-bottom')}" min="0" max="500" step="4"></label></div>
                    <div><label>Left (px)<br><input type="number" id="md-left"  value="${get('--page-margin-left')}"   min="0" max="500" step="4"></label></div>
                    <div><label>Right (px)<br><input type="number" id="md-right" value="${get('--page-margin-right')}" min="0" max="500" step="4"></label></div>
                </div>
                <div class="margins-dialog-actions">
                    <button id="md-cancel">Cancel</button>
                    <button id="md-ok" class="btn-primary">Apply</button>
                </div>
            </div>
        `;
        document.body.appendChild(overlay);
        overlay.querySelector('#md-cancel').addEventListener('click', () => overlay.remove());
        overlay.querySelector('#md-ok').addEventListener('click', () => {
            setRootVar('--page-margin-top',    overlay.querySelector('#md-top').value    + 'px');
            setRootVar('--page-margin-bottom', overlay.querySelector('#md-bot').value    + 'px');
            setRootVar('--page-margin-left',   overlay.querySelector('#md-left').value   + 'px');
            setRootVar('--page-margin-right',  overlay.querySelector('#md-right').value  + 'px');
            overlay.remove();
            pushUndoSnapshot();
        });
        overlay.addEventListener('click', e => { if (e.target === overlay) overlay.remove(); });
    }

    // Add "Custom…" button to picker dynamically
    const picker = document.getElementById('margins-picker');
    if (picker) {
        const customBtn = document.createElement('button');
        customBtn.className = 'fmenu-item';
        customBtn.textContent = 'Custom Margins…';
        customBtn.addEventListener('mousedown', e => {
            e.preventDefault();
            picker.hidden = true;
            openMarginsDialog();
        });
        picker.appendChild(customBtn);
    }
})();

// ── H04: Orientation ──────────────────────────────────────────────────────────
document.querySelectorAll('#orientation-picker [data-orientation]').forEach(btn => {
    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const isLandscape = btn.dataset.orientation === 'landscape';
        const editorEl = document.getElementById('editor');
        editorEl.classList.toggle('landscape', isLandscape);
        if (isLandscape) {
            setRootVar('--page-w', '1056px');
        } else {
            setRootVar('--page-w', '816px');
        }
        document.getElementById('orientation-picker').hidden = true;
        updatePageCount();
    });
});

// ── H05: Page Size ────────────────────────────────────────────────────────────
const PAGE_SIZES = {
    letter:    { w: '816px',  h: '1056px', name: 'US Letter' },
    legal:     { w: '816px',  h: '1344px', name: 'Legal' },
    a4:        { w: '794px',  h: '1123px', name: 'A4' },
    executive: { w: '696px',  h: '1008px', name: 'Executive' },
};
document.querySelectorAll('#size-picker [data-size]').forEach(btn => {
    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const sz = PAGE_SIZES[btn.dataset.size];
        if (!sz) return;
        setRootVar('--page-w', sz.w);
        const editorEl = document.getElementById('editor');
        editorEl.style.minHeight = sz.h;
        document.getElementById('size-picker').hidden = true;
        updatePageCount();
    });
});

// ── H06: Columns ──────────────────────────────────────────────────────────────
document.querySelectorAll('#columns-picker [data-columns]').forEach(btn => {
    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const n = parseInt(btn.dataset.columns, 10);
        setRootVar('--page-cols', String(n));
        document.getElementById('columns-picker').hidden = true;
        updatePageCount();
    });
});

// ── H12: Breaks ───────────────────────────────────────────────────────────────
document.querySelectorAll('#breaks-picker [data-break]').forEach(btn => {
    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const type = btn.dataset.break;
        editor.focus();
        const sel = window.getSelection();
        if (sel && sel.rangeCount) {
            const range = sel.getRangeAt(0);
            range.collapse(false);
            const brk = document.createElement('div');
            brk.className = type === 'page' ? 'page-break-ruler' : 'column-break-sentinel';
            brk.contentEditable = 'false';
            brk.setAttribute('data-break', type);
            brk.textContent = type === 'page' ? '— Page Break —' : '— Column Break —';
            range.insertNode(brk);
            // Move cursor after break
            const newRange = document.createRange();
            newRange.setStartAfter(brk);
            newRange.collapse(true);
            sel.removeAllRanges();
            sel.addRange(newRange);
        }
        document.getElementById('breaks-picker').hidden = true;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    });
});

// ── Line Numbers ──────────────────────────────────────────────────────────────
document.querySelectorAll('#line-numbers-picker [data-lineno]').forEach(btn => {
    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const mode = btn.dataset.lineno;
        editor.classList.toggle('show-line-numbers', mode !== 'none');
        document.getElementById('line-numbers-picker').hidden = true;
    });
});

// ── H31: Hyphenation ──────────────────────────────────────────────────────────
document.querySelectorAll('#hyphenation-picker [data-hyphen]').forEach(btn => {
    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        editor.style.hyphens    = btn.dataset.hyphen === 'auto' ? 'auto' : 'none';
        editor.style.webkitHyphens = editor.style.hyphens;
        document.getElementById('hyphenation-picker').hidden = true;
    });
});

// ── Watermark ─────────────────────────────────────────────────────────────────
document.querySelectorAll('#watermark-picker [data-watermark]').forEach(btn => {
    btn.addEventListener('mousedown', e => {
        e.preventDefault();
        const text = btn.dataset.watermark;
        if (!text) {
            setRootVar('--watermark-text', "''");
            setRootVar('--watermark-opacity', '0');
        } else {
            setRootVar('--watermark-text', `"${text}"`);
            setRootVar('--watermark-opacity', '0.08');
        }
        document.getElementById('watermark-picker').hidden = true;
    });
});

// ── Page Color (Layout panel) ─────────────────────────────────────────────────
document.getElementById('page-color-picker')?.addEventListener('input', e => {
    setRootVar('--page-bg-fill', e.target.value);
});

// ── H10: Header / Footer zone management ─────────────────────────────────────
(function() {
    const headerZone = document.getElementById('doc-header');
    const footerZone = document.getElementById('doc-footer');

    function toggleZone(zone) {
        if (!zone) return;
        zone.classList.toggle('hidden-zone');
        if (!zone.classList.contains('hidden-zone')) zone.focus();
    }

    // Insert panel buttons
    document.getElementById('btn-insert-header')?.addEventListener('click', () => toggleZone(headerZone));
    document.getElementById('btn-insert-footer')?.addEventListener('click', () => toggleZone(footerZone));

    // Page number field insertion (H11)
    document.getElementById('btn-insert-page-num')?.addEventListener('click', () => {
        const zone = footerZone;
        zone.classList.remove('hidden-zone');
        zone.focus();
        const field = document.createElement('span');
        field.className = 'field-page';
        field.contentEditable = 'false';
        field.title = '{{page}} — page number field';
        const sel = window.getSelection();
        if (sel && sel.rangeCount && zone.contains(sel.anchorNode)) {
            sel.getRangeAt(0).insertNode(field);
        } else {
            zone.appendChild(field);
        }
    });

    // {{page}}, {{date}}, {{title}} token resolution on render
    function resolveFieldTokens(zone) {
        if (!zone) return;
        zone.querySelectorAll('[data-token]').forEach(span => {
            const token = span.dataset.token;
            if (token === 'page') span.textContent = '1'; // static; CSS counter handles print
            else if (token === 'date') span.textContent = new Date().toLocaleDateString();
            else if (token === 'title') span.textContent = currentDocName;
        });
    }
    // Resolve on input in zones
    headerZone?.addEventListener('input', () => resolveFieldTokens(headerZone));
    footerZone?.addEventListener('input', () => resolveFieldTokens(footerZone));
})();

// ════════════════════════════════════════════════════════════════════════════
// Sprint 2.5 — Design Tab Handlers (D01–D09)
// ════════════════════════════════════════════════════════════════════════════

// ── D02, D03, D04, D05: Document Themes, Styles, Formatting ────────────────────────────────────
(function() {
    const DESIGN_SETTINGS_KEY = 'marksmen-design-v2';
    let designState = { theme: null, spacing: null, styleSet: null, colorOverride: null, fontOverride: null };
    try { const s = localStorage.getItem(DESIGN_SETTINGS_KEY); if (s) designState = { ...designState, ...JSON.parse(s) }; } catch {}

    function applyDesignState() {
        // Apply theme
        if (designState.theme) document.body.dataset.theme = designState.theme;
        else delete document.body.dataset.theme;
        // Apply spacing
        if (designState.spacing) document.body.dataset.spacing = designState.spacing;
        else delete document.body.dataset.spacing;
        // Apply style set
        if (designState.styleSet) document.body.dataset.styleSet = designState.styleSet;
        else delete document.body.dataset.styleSet;
        // Apply formatting overrides
        if (designState.colorOverride) document.body.dataset.colorOverride = designState.colorOverride;
        else delete document.body.dataset.colorOverride;
        
        if (designState.fontOverride) document.body.dataset.fontOverride = designState.fontOverride;
        else delete document.body.dataset.fontOverride;

        // Update UI
        document.querySelectorAll('.theme-card[data-theme]').forEach(c =>
            c.classList.toggle('theme-card--active', c.dataset.theme === designState.theme));
        document.querySelectorAll('.style-card[data-style]').forEach(c =>
            c.classList.toggle('theme-card--active', c.dataset.style === designState.styleSet));
        document.querySelectorAll('.spacing-preset-btn').forEach(b =>
            b.classList.toggle('active', b.dataset.spacing === (designState.spacing ?? '')));
            
        const cPicker = document.getElementById('design-color-picker');
        if (cPicker) cPicker.value = designState.colorOverride || '';
        const fPicker = document.getElementById('design-font-picker');
        if (fPicker) fPicker.value = designState.fontOverride || '';
    }

    applyDesignState();

    document.querySelectorAll('.theme-card[data-theme]').forEach(card => {
        card.addEventListener('click', () => {
            const t = card.dataset.theme;
            designState.theme = designState.theme === t ? null : t;
            applyDesignState();
        });
    });

    document.querySelectorAll('.style-card[data-style]').forEach(card => {
        card.addEventListener('click', () => {
            const t = card.dataset.style;
            designState.styleSet = designState.styleSet === t ? null : t;
            applyDesignState();
        });
    });

    document.getElementById('design-color-picker')?.addEventListener('change', (e) => {
        designState.colorOverride = e.target.value || null;
        applyDesignState();
    });

    document.getElementById('design-font-picker')?.addEventListener('change', (e) => {
        designState.fontOverride = e.target.value || null;
        applyDesignState();
    });

    // D06: Paragraph spacing presets
    document.querySelectorAll('.spacing-preset-btn[data-spacing]').forEach(btn => {
        btn.addEventListener('click', () => {
            designState.spacing = btn.dataset.spacing || null;
            applyDesignState();
        });
    });

    // D07: Page color from Design tab
    document.getElementById('design-page-color')?.addEventListener('input', e => {
        setRootVar('--page-bg-fill', e.target.value);
    });

    // D08: Page borders
    document.getElementById('btn-design-page-borders')?.addEventListener('click', () => {
        const page = document.querySelector('.document-page');
        if (!page) return;
        const has = page.style.outline && page.style.outline !== 'none';
        page.style.outline = has ? 'none' : '2px solid var(--border)';
        showToast(has ? 'Page border removed' : 'Page border applied');
    });

    // D09: Set as Default
    document.getElementById('btn-design-set-default')?.addEventListener('click', () => {
        localStorage.setItem(DESIGN_SETTINGS_KEY, JSON.stringify(designState));
        showToast('Design saved as default for new documents');
    });
})();

// ── H01, H02: Layout Paragraph Spacing ────────────────────────────────────────────────────────
(function() {
    function applyBlockStyle(cssProp, value) {
        if (!window.getSelection().rangeCount) return;
        
        const sel = window.getSelection();
        const range = sel.getRangeAt(0);
        
        let startBlock = getEnclosingBlock(range.startContainer);
        let endBlock = getEnclosingBlock(range.endContainer);
        
        if (!startBlock || !editor.contains(startBlock)) startBlock = editor.firstChild;
        if (!endBlock || !editor.contains(endBlock)) endBlock = editor.lastChild;
        
        if (startBlock === endBlock && startBlock) {
            startBlock.style[cssProp] = value;
        } else if (startBlock && endBlock) {
            let curr = startBlock;
            while (curr && curr !== endBlock.nextSibling) {
                if (curr.nodeType === Node.ELEMENT_NODE) {
                    curr.style[cssProp] = value;
                }
                curr = curr.nextSibling;
            }
        }
        editor.dispatchEvent(new Event('input'));
    }

    function getEnclosingBlock(node) {
        if (!node) return null;
        if (node.nodeType === Node.ELEMENT_NODE && ['P', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'LI'].includes(node.tagName)) return node;
        return node.closest ? node.closest('p, h1, h2, h3, h4, h5, h6, li') : getEnclosingBlock(node.parentNode);
    }

    document.getElementById('layout-line-spacing')?.addEventListener('change', e => {
        const val = e.target.value;
        if (val) applyBlockStyle('lineHeight', val);
    });

    document.getElementById('layout-space-before')?.addEventListener('change', e => {
        const val = e.target.value;
        if (val !== '') applyBlockStyle('marginTop', val + 'pt');
    });

    document.getElementById('layout-space-after')?.addEventListener('change', e => {
        const val = e.target.value;
        if (val !== '') applyBlockStyle('marginBottom', val + 'pt');
    });
})();

// ════════════════════════════════════════════════════════════════════════════
// Sprint 2.6 — View Tab Enhancements & Remaining Home Items
// ════════════════════════════════════════════════════════════════════════════

// ── H07: Named styles (Title, Subtitle, Body Text, Emphasis, Intense Quote, Caption) ─
(function() {
    // CSS properties per named style — applied by wrapping selection in a span or
    // converting the containing block. For block-level styles we formatBlock; for
    // inline semantic styles we wrap the selection in a span with a class.
    const NAMED_STYLES = {
        'title': {
            block: true, tag: 'h1',
            css: { fontSize: '2em', fontWeight: '700', letterSpacing: '-0.02em', fontStyle: 'normal' },
            className: 'doc-title',
        },
        'subtitle': {
            block: true, tag: 'p',
            css: { fontSize: '1.1em', fontWeight: '400', color: '#6c757d', fontStyle: 'italic', marginTop: '-0.5em' },
            className: 'doc-subtitle',
        },
        'body-text': {
            block: true, tag: 'p',
            css: { fontSize: '11pt', lineHeight: '1.6', fontStyle: 'normal', fontWeight: '400' },
            className: '',
        },
        'emphasis': {
            block: false,
            css: { fontStyle: 'italic', color: 'var(--accent)' },
            className: 'doc-emphasis',
        },
        'intense-quote': {
            block: true, tag: 'blockquote',
            css: { fontStyle: 'italic', fontWeight: '600', borderLeftWidth: '4px', color: 'var(--accent)' },
            className: 'doc-intense-quote',
        },
        'caption': {
            block: false,
            css: { fontSize: '9pt', fontStyle: 'italic', color: '#868e96' },
            className: 'doc-caption',
        },
    };

    document.querySelectorAll('.style-card[data-style-name]').forEach(card => {
        card.addEventListener('mousedown', e => {
            e.preventDefault();
            const name = card.dataset.styleName;
            const def = NAMED_STYLES[name];
            if (!def) return;
            editor.focus();
            const sel = window.getSelection();
            if (!sel || !sel.rangeCount) return;
            if (def.block && def.tag) {
                // Convert containing block element
                document.execCommand('formatBlock', false, def.tag);
                // Then apply className to the resulting block
                if (def.className) {
                    const anchor = sel.anchorNode?.parentElement;
                    const block = anchor?.closest(def.tag) || anchor;
                    if (block) { block.className = def.className; }
                }
            } else {
                // Inline: wrap selection in a span
                const range = sel.getRangeAt(0);
                const span = document.createElement('span');
                if (def.className) span.className = def.className;
                Object.assign(span.style, def.css);
                try { range.surroundContents(span); } catch(_) {
                    // If surroundContents fails (cross-element), insert text node
                    const txt = range.toString();
                    span.textContent = txt;
                    range.deleteContents();
                    range.insertNode(span);
                }
            }
            editor.dispatchEvent(new Event('input'));
            pushUndoSnapshot();
        });
    });
})();

// ── H23: Ctrl+J — Justify Full shortcut ───────────────────────────────────────
// (Ctrl+J is not wired in the existing keyboard handler; add it here)
document.addEventListener('keydown', e => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'j') {
        e.preventDefault();
        document.execCommand('justifyFull');
        editor.focus();
    }
}, true); // capture phase to avoid duplicate with existing listener

// ── H16: Multi-level list — Tab/Shift+Tab inside list items ──────────────────
editor.addEventListener('keydown', e => {
    if (e.key !== 'Tab') return;
    const sel = window.getSelection();
    if (!sel || !sel.rangeCount) return;
    const li = sel.anchorNode?.parentElement?.closest('li');
    if (!li) return;
    e.preventDefault();
    if (e.shiftKey) {
        document.execCommand('outdent');
    } else {
        document.execCommand('indent');
    }
    pushUndoSnapshot();
}, true);

// ── V01: View Mode Switcher (Read / Print / Web / Draft) ──────────────────────
(function() {
    let currentViewMode = 'print';

    const VIEW_BODY_CLASSES = {
        print: [],
        read:  ['view-read'],
        web:   ['view-web'],
        draft: ['view-draft'],
    };

    function setViewMode(mode) {
        // Remove all view classes
        Object.values(VIEW_BODY_CLASSES).flat().forEach(c => document.body.classList.remove(c));
        // Apply new
        (VIEW_BODY_CLASSES[mode] || []).forEach(c => document.body.classList.add(c));
        currentViewMode = mode;

        // Update active button state
        document.querySelectorAll('.view-mode-btn').forEach(btn => {
            btn.classList.toggle('rbtn--active-toggle', btn.dataset.view === mode);
        });

        // Read mode: disable editing
        const isEditable = mode !== 'read';
        editor.contentEditable = isEditable ? 'true' : 'false';
        if (!isEditable) editor.blur();

        // Draft mode: hide images
        editor.classList.toggle('view-draft-active', mode === 'draft');
    }

    document.querySelectorAll('.view-mode-btn').forEach(btn => {
        btn.addEventListener('click', () => setViewMode(btn.dataset.view));
    });

    // Boot in print mode
    setViewMode('print');
})();

// ── V03: Gridlines toggle ─────────────────────────────────────────────────────
document.getElementById('btn-gridlines')?.addEventListener('click', function() {
    const canvas = document.getElementById('page-canvas');
    const active = this.classList.toggle('rbtn--active-toggle');
    canvas.classList.toggle('show-gridlines', active);
});

// ── V04: Navigation Pane — toggle existing sidebar to outline tab ─────────────
document.getElementById('btn-nav-pane')?.addEventListener('click', function() {
    const sidebar = document.getElementById('sidebar');
    const stab = document.getElementById('stab-outline');
    if (!sidebar) return;
    const isCollapsed = sidebar.classList.contains('collapsed');
    if (isCollapsed) {
        sidebar.classList.remove('collapsed');
        // Switch to outline tab
        document.querySelectorAll('.stab').forEach(t => t.classList.remove('stab--active'));
        stab?.classList.add('stab--active');
        document.querySelectorAll('.sidebar-pane').forEach(p => p.hidden = true);
        document.getElementById('outline-list')?.closest('.sidebar-pane')?.removeAttribute('hidden');
        this.classList.add('rbtn--active-toggle');
    } else {
        sidebar.classList.add('collapsed');
        this.classList.remove('rbtn--active-toggle');
    }
});

// ── V05: Zoom dialog ──────────────────────────────────────────────────────────
document.getElementById('btn-zoom-dialog')?.addEventListener('click', function() {
    // Build and show inline zoom dialog
    const existing = document.getElementById('zoom-dialog-overlay');
    if (existing) { existing.remove(); return; }
    const overlay = document.createElement('div');
    overlay.id = 'zoom-dialog-overlay';
    overlay.style.cssText = 'position:fixed;inset:0;background:rgba(0,0,0,0.3);z-index:8000;display:flex;align-items:center;justify-content:center;';
    const canvas = document.getElementById('page-canvas');
    const currentScale = parseFloat(canvas.style.transform?.replace('scale(','') ?? '1') || 1;
    overlay.innerHTML = `
        <div style="background:var(--ribbon-bg);border:1px solid var(--border);border-radius:8px;padding:20px 24px;min-width:260px;box-shadow:0 8px 32px rgba(0,0,0,0.2);display:flex;flex-direction:column;gap:12px;">
            <div style="font-size:14px;font-weight:600;color:var(--text-primary);">Zoom</div>
            <div style="display:flex;gap:8px;flex-wrap:wrap;">
                <button class="zoom-preset-btn" data-z="0.75">75%</button>
                <button class="zoom-preset-btn" data-z="1">100%</button>
                <button class="zoom-preset-btn" data-z="1.25">125%</button>
                <button class="zoom-preset-btn" data-z="1.5">150%</button>
                <button class="zoom-preset-btn" data-z="2">200%</button>
            </div>
            <div style="display:flex;align-items:center;gap:8px;">
                <label style="font-size:11px;color:var(--text-secondary);">Custom %</label>
                <input id="zoom-custom-input" type="number" min="10" max="500" value="${Math.round(currentScale * 100)}"
                       style="width:70px;padding:4px 8px;border:1px solid var(--border);border-radius:4px;background:var(--chrome-bg);color:var(--text-primary);font-size:12px;">
                <button id="zoom-apply-btn" style="padding:4px 12px;border-radius:4px;background:var(--accent);color:white;border:none;cursor:pointer;font-size:12px;">Apply</button>
            </div>
            <div style="display:flex;justify-content:flex-end;">
                <button id="zoom-close-btn" style="padding:4px 12px;border-radius:4px;border:1px solid var(--border);background:var(--chrome-bg);color:var(--text-primary);font-size:12px;cursor:pointer;">Close</button>
            </div>
        </div>
    `;
    document.body.appendChild(overlay);
    // Style preset buttons
    overlay.querySelectorAll('.zoom-preset-btn').forEach(btn => {
        btn.style.cssText = 'padding:4px 10px;border-radius:4px;border:1px solid var(--border);background:var(--chrome-bg);color:var(--text-primary);font-size:12px;cursor:pointer;';
        btn.addEventListener('click', () => { zoomPage(parseFloat(btn.dataset.z) * 100); overlay.remove(); });
    });
    overlay.querySelector('#zoom-apply-btn').addEventListener('click', () => {
        const pct = parseInt(overlay.querySelector('#zoom-custom-input').value, 10);
        if (pct >= 10 && pct <= 500) { zoomPage(pct); overlay.remove(); }
    });
    overlay.querySelector('#zoom-close-btn').addEventListener('click', () => overlay.remove());
    overlay.addEventListener('click', e => { if (e.target === overlay) overlay.remove(); });
});

// ── V03: Gridlines CSS injection (add rule if not already present) ─────────────
(function() {
    const styleEl = document.createElement('style');
    styleEl.id = 'gridlines-style';
    styleEl.textContent = `
        .page-canvas.show-gridlines #editor {
            background-image:
                linear-gradient(to right, rgba(37,99,235,0.06) 1px, transparent 1px),
                linear-gradient(to bottom, rgba(37,99,235,0.06) 1px, transparent 1px);
            background-size: 40px 40px;
        }
        /* V01: View mode body classes */
        body.view-read .ribbon { display: none; }
        body.view-read #find-bar { display: none; }
        body.view-read .page-canvas { padding-top: 24px; }
        body.view-web #editor {
            max-width: 100%;
            width: 100%;
            border-radius: 0;
            box-shadow: none;
            min-height: 100vh;
        }
        body.view-web .page-canvas { padding: 0; }
        body.view-draft #editor img { display: none; }
        body.view-draft #editor .page-break-ruler { display: none; }
        /* H07: Named style classes */
        .document-page .doc-title { font-size: 2em; font-weight: 700; letter-spacing: -0.02em; border-bottom: 2px solid var(--border); padding-bottom: 8px; }
        .document-page .doc-subtitle { font-size: 1.1em; color: #6c757d; font-style: italic; margin-top: -0.4em; }
        .document-page .doc-emphasis { font-style: italic; color: var(--accent); }
        .document-page .doc-intense-quote { font-style: italic; font-weight: 600; border-left: 4px solid var(--accent); padding-left: 12px; color: var(--accent); }
        .document-page .doc-caption { font-size: 9pt; font-style: italic; color: #868e96; display: block; text-align: center; margin-top: 4px; }
        @media print {
            body.view-read .ribbon { display: none; }
        }
    `;
    document.head.appendChild(styleEl);
})();

// ── Ruler toggle ──────────────────────────────────────────────────────────────
document.getElementById('btn-ruler')?.addEventListener('click', function() {
    const active = this.classList.toggle('rbtn--active-toggle');
    document.getElementById('page-canvas')?.classList.toggle('show-ruler', active);
    showToast(active ? 'Ruler visible' : 'Ruler hidden');
});

// ════════════════════════════════════════════════════════════════════════════
// Sprint 4 + Polish — Remaining Items
// ════════════════════════════════════════════════════════════════════════════

// ── M02: Autocorrect ──────────────────────────────────────────────────────────
// Fires on space/enter/punctuation; replaces last word with corrected form.
(function() {
    const AUTOCORRECT_MAP = {
        '--':   '\u2014',   // em dash
        '...':  '\u2026',  // ellipsis
        '(c)':  '\u00a9',  // copyright
        '(r)':  '\u00ae',  // registered
        '(tm)': '\u2122',  // trademark
        '<->':  '\u2194',  // left-right arrow
        '->':   '\u2192',  // right arrow
        '<-':   '\u2190',  // left arrow
        '!=':   '\u2260',  // not equal
        '>=':   '\u2265',  // >=
        '<=':   '\u2264',  // <=
        '1/2':  '\u00bd',  // ½
        '1/4':  '\u00bc',  // ¼
        '3/4':  '\u00be',  // ¾
    };
    // Max key length for backward look-ahead
    const MAX_KEY = Math.max(...Object.keys(AUTOCORRECT_MAP).map(k => k.length));

    editor.addEventListener('keydown', e => {
        const triggers = new Set([' ', 'Enter', '.', ',', '!', '?', ';', ':']);
        if (!triggers.has(e.key)) return;

        const sel = window.getSelection();
        if (!sel || !sel.rangeCount) return;
        const range = sel.getRangeAt(0);
        if (!range.collapsed) return;

        const node = range.startContainer;
        if (node.nodeType !== 3) return; // text node only

        const text = node.textContent;
        const offset = range.startOffset;

        // Scan backward up to MAX_KEY chars for a match
        for (let len = Math.min(MAX_KEY, offset); len >= 1; len--) {
            const candidate = text.slice(offset - len, offset);
            if (AUTOCORRECT_MAP[candidate]) {
                // Delete the candidate and insert replacement
                e.preventDefault();
                const replacement = AUTOCORRECT_MAP[candidate] + (e.key === ' ' ? ' ' : e.key === 'Enter' ? '' : e.key);
                const newRange = document.createRange();
                newRange.setStart(node, offset - len);
                newRange.setEnd(node, offset);
                sel.removeAllRanges();
                sel.addRange(newRange);
                document.execCommand('insertText', false, replacement);
                // Re-insert the trigger character for Enter (let default handle newline)
                if (e.key === 'Enter') {
                    document.execCommand('insertParagraph');
                }
                editor.dispatchEvent(new Event('input'));
                return;
            }
        }

        // Smart quotes: wrap " in curly pairs, ' in curly pairs
        if (e.key === '"' || e.key === "'") {
            e.preventDefault();
            const before = offset > 0 ? text[offset - 1] : ' ';
            const isOpening = /\s/.test(before) || offset === 0;
            const replacement = e.key === '"'
                ? (isOpening ? '\u201c' : '\u201d')
                : (isOpening ? '\u2018' : '\u2019');
            document.execCommand('insertText', false, replacement);
        }
    }, true);
})();

// ── L07: Drag-drop file open ──────────────────────────────────────────────────
(function() {
    const workspace = document.getElementById('workspace') || document.body;
    workspace.addEventListener('dragover', e => {
        e.preventDefault();
        e.dataTransfer.dropEffect = 'copy';
        workspace.classList.add('drag-over');
    });
    workspace.addEventListener('dragleave', e => {
        if (!workspace.contains(e.relatedTarget)) workspace.classList.remove('drag-over');
    });
    workspace.addEventListener('drop', async e => {
        e.preventDefault();
        workspace.classList.remove('drag-over');
        const files = [...e.dataTransfer.files];
        if (!files.length) return;
        const f = files[0];
        const supported = ['.md', '.txt', '.html', '.docx', '.odt', '.pdf', '.rtf', '.typ'];
        const ext = f.name.slice(f.name.lastIndexOf('.')).toLowerCase();
        if (!supported.includes(ext)) {
            showToast(`Unsupported file type: ${ext}`);
            return;
        }
        try {
            if (window.__TAURI__) {
                // Tauri: use file path from DataTransferItem
                const path = f.path || (e.dataTransfer.items?.[0] && await new Promise(res => {
                    e.dataTransfer.items[0].getAsString(res);
                }));
                if (path) {
                    await invoke('import_file', { path });
                    showToast(`Opened: ${f.name}`);
                }
            } else {
                // Browser fallback: read as text
                const text = await f.text();
                editor.innerHTML = text;
                editor.dispatchEvent(new Event('input'));
                showToast(`Loaded: ${f.name}`);
            }
        } catch(err) {
            showToast(`Failed to open file: ${err}`);
            console.error(err);
        }
    });
})();

// ── L10: Outline heading highlight on scroll (IntersectionObserver) ───────────
(function() {
    if (!('IntersectionObserver' in window)) return;
    let ticking = false;
    function highlightOutline() {
        const headings = [...editor.querySelectorAll('h1,h2,h3,h4,h5,h6')];
        if (!headings.length) return;
        const editorTop = editor.getBoundingClientRect().top;
        // Find the last heading that has scrolled above the 30% viewport mark
        let current = headings[0];
        for (const h of headings) {
            const top = h.getBoundingClientRect().top - editorTop;
            if (top < window.innerHeight * 0.3) current = h;
        }
        const outlineList = document.getElementById('outline-list');
        if (!outlineList) return;
        outlineList.querySelectorAll('li, a').forEach(el => el.classList.remove('outline-current'));
        const id = current.id || current.textContent.trim().toLowerCase().replace(/\s+/g, '-').slice(0, 40);
        if (!current.id) current.id = id;
        const link = outlineList.querySelector(`a[href="#${id}"]`);
        link?.classList.add('outline-current');
        link?.closest('li')?.classList.add('outline-current');
    }

    // Throttled scroll handler
    const scrollHost = document.getElementById('workspace') || window;
    scrollHost.addEventListener('scroll', () => {
        if (!ticking) {
            requestAnimationFrame(() => { highlightOutline(); ticking = false; });
            ticking = true;
        }
    }, { passive: true });
    // Initial highlight
    setTimeout(highlightOutline, 500);
})();

// ── M22: Table styles gallery ─────────────────────────────────────────────────
(function() {
    const TABLE_STYLES = [
        { id: 'ts-plain',        label: 'Plain' },
        { id: 'ts-striped',      label: 'Striped' },
        { id: 'ts-bordered',     label: 'Bordered' },
        { id: 'ts-header-blue',  label: 'Header Blue' },
        { id: 'ts-compact',      label: 'Compact' },
        { id: 'ts-modern',       label: 'Modern' },
        { id: 'ts-grid-light',   label: 'Grid Light' },
        { id: 'ts-dark-accent',  label: 'Dark Accent' },
    ];

    // Build the picker dropdown (inserted into body, repositioned on demand)
    const picker = document.createElement('div');
    picker.id = 'table-style-picker';
    picker.hidden = true;
    picker.style.cssText = 'position:fixed;z-index:9999;background:var(--ribbon-bg);border:1px solid var(--border);border-radius:6px;padding:8px;display:flex;flex-wrap:wrap;gap:6px;width:220px;box-shadow:0 4px 16px rgba(0,0,0,0.2);';
    TABLE_STYLES.forEach(({ id, label }) => {
        const btn = document.createElement('button');
        btn.dataset.tableStyle = id;
        btn.style.cssText = 'width:90px;padding:6px;font-size:10px;border:1px solid var(--border);border-radius:4px;background:var(--chrome-bg);color:var(--text-primary);cursor:pointer;text-align:center;';
        btn.textContent = label;
        btn.addEventListener('mousedown', e => {
            e.preventDefault();
            if (!_ctxTable) { picker.hidden = true; return; }
            // Remove all existing table styles
            TABLE_STYLES.forEach(s => _ctxTable.classList.remove(s.id));
            if (id !== 'ts-plain') _ctxTable.classList.add(id);
            picker.hidden = true;
            editor.dispatchEvent(new Event('input'));
            pushUndoSnapshot();
        });
        picker.appendChild(btn);
    });
    document.body.appendChild(picker);

    // Close on outside click
    document.addEventListener('mousedown', e => {
        if (!picker.hidden && !picker.contains(e.target)) picker.hidden = true;
    });

    // Expose opener — called from table context menu "Style" option
    window._openTableStylePicker = function(table, x, y) {
        _ctxTable = table;
        picker.style.left = x + 'px';
        picker.style.top  = y + 'px';
        picker.hidden = false;
    };

    // Wire to table context menu if it has a "Style" item
    const ctxStyle = document.getElementById('ctx-table-style');
    if (ctxStyle) {
        ctxStyle.addEventListener('click', () => {
            if (!_ctxTable) return;
            const rect = ctxStyle.getBoundingClientRect();
            window._openTableStylePicker(_ctxTable, rect.right + 4, rect.top);
        });
    }
})();

// ── H29: Full categorized symbol picker ───────────────────────────────────────
(function() {
    const SYMBOL_CATEGORIES = {
        'Latin': [
            ['À','A with grave'],['Á','A with acute'],['Â','A with circumflex'],['Ã','A with tilde'],
            ['Ä','A with diaeresis'],['Å','A with ring'],['Æ','AE ligature'],['Ç','C with cedilla'],
            ['È','E with grave'],['É','E with acute'],['Ê','E with circumflex'],['Ë','E with diaeresis'],
            ['Î','I with circumflex'],['Ï','I with diaeresis'],['Ñ','N with tilde'],['Ô','O with circumflex'],
            ['Ö','O with diaeresis'],['Ø','O with stroke'],['Ù','U with grave'],['Ú','U with acute'],
            ['Û','U with circumflex'],['Ü','U with diaeresis'],['ß','German sharp s'],['à','a with grave'],
            ['á','a with acute'],['â','a with circumflex'],['ã','a with tilde'],['ä','a with diaeresis'],
            ['å','a with ring'],['æ','ae ligature'],['ç','c with cedilla'],['è','e with grave'],
        ],
        'Greek': [
            ['α','alpha'],['β','beta'],['γ','gamma'],['δ','delta'],['ε','epsilon'],['ζ','zeta'],
            ['η','eta'],['θ','theta'],['ι','iota'],['κ','kappa'],['λ','lambda'],['μ','mu'],
            ['ν','nu'],['ξ','xi'],['ο','omicron'],['π','pi'],['ρ','rho'],['σ','sigma'],
            ['τ','tau'],['υ','upsilon'],['φ','phi'],['χ','chi'],['ψ','psi'],['ω','omega'],
            ['Α','Alpha'],['Β','Beta'],['Γ','Gamma'],['Δ','Delta'],['Ε','Epsilon'],['Θ','Theta'],
            ['Λ','Lambda'],['Π','Pi'],['Σ','Sigma'],['Φ','Phi'],['Ψ','Psi'],['Ω','Omega'],
        ],
        'Math': [
            ['∑','sum'],['∏','product'],['∫','integral'],['∂','partial derivative'],['∇','nabla'],
            ['√','square root'],['∛','cube root'],['∞','infinity'],['∅','empty set'],['∈','element of'],
            ['∉','not element of'],['⊂','subset'],['⊃','superset'],['∪','union'],['∩','intersection'],
            ['≠','not equal'],['≈','approximately equal'],['≡','identical'],['≤','less or equal'],
            ['≥','greater or equal'],['±','plus minus'],['×','times'],['÷','division'],['°','degree'],
            ['ℝ','real numbers'],['ℤ','integers'],['ℕ','natural numbers'],['ℂ','complex numbers'],
            ['ℚ','rational numbers'],['ℏ','h-bar'],['∀','for all'],['∃','there exists'],
        ],
        'Arrows': [
            ['→','right arrow'],['←','left arrow'],['↑','up arrow'],['↓','down arrow'],
            ['↔','left-right arrow'],['↕','up-down arrow'],['⇒','right double arrow'],
            ['⇐','left double arrow'],['⇔','left-right double arrow'],['↗','north-east arrow'],
            ['↘','south-east arrow'],['↙','south-west arrow'],['↖','north-west arrow'],
            ['⟶','long right arrow'],['⟵','long left arrow'],['⟷','long left-right arrow'],
        ],
        'Currency': [
            ['€','euro'],['£','pound'],['¥','yen'],['¢','cent'],['₹','rupee'],['₽','ruble'],
            ['₩','won'],['₪','shekel'],['₺','lira'],['₫','dong'],['$','dollar'],['₿','bitcoin'],
        ],
        'Punctuation': [
            ['©','copyright'],['®','registered'],['™','trademark'],['§','section'],['¶','pilcrow'],
            ['†','dagger'],['‡','double dagger'],['•','bullet'],['·','middle dot'],['…','ellipsis'],
            ['\u2014','em dash'],['\u2013','en dash'],['‹','single left angle'],['›','single right angle'],
            ['«','left guillemet'],['»','right guillemet'],['\u201c','left double quote'],['\u201d','right double quote'],
        ],
    };

    let pickerInitialized = false;
    const symbolPicker = document.getElementById('symbol-picker');
    if (!symbolPicker) return;

    function initFullSymbolPicker() {
        if (pickerInitialized) return;
        pickerInitialized = true;

        // Replace picker content with full UI
        symbolPicker.style.cssText = 'position:fixed;z-index:9999;background:var(--ribbon-bg);border:1px solid var(--border);border-radius:8px;padding:8px;width:280px;max-height:360px;overflow:hidden;display:flex;flex-direction:column;gap:6px;box-shadow:0 4px 16px rgba(0,0,0,0.2);';

        // Filter input
        const filterWrap = document.createElement('div');
        filterWrap.style.cssText = 'display:flex;gap:4px;';
        const filterInput = document.createElement('input');
        filterInput.id = 'symbol-filter';
        filterInput.placeholder = 'Search symbols…';
        filterInput.autocomplete = 'off';
        filterInput.style.cssText = 'flex:1;padding:4px 8px;border:1px solid var(--border);border-radius:4px;font-size:11px;background:var(--chrome-bg);color:var(--text-primary);';
        filterWrap.appendChild(filterInput);
        symbolPicker.appendChild(filterWrap);

        // Category tabs
        const tabBar = document.createElement('div');
        tabBar.style.cssText = 'display:flex;gap:2px;flex-wrap:wrap;';
        const cats = Object.keys(SYMBOL_CATEGORIES);
        let activeCat = cats[0];
        const tabBtns = {};
        cats.forEach(cat => {
            const t = document.createElement('button');
            t.textContent = cat;
            t.style.cssText = 'padding:2px 6px;border-radius:3px;border:1px solid var(--border);font-size:10px;cursor:pointer;background:var(--chrome-bg);color:var(--text-primary);';
            t.addEventListener('click', () => {
                activeCat = cat;
                Object.values(tabBtns).forEach(b => b.style.background = 'var(--chrome-bg)');
                t.style.background = 'var(--accent)';
                t.style.color = '#fff';
                renderGrid(cat);
                filterInput.value = '';
            });
            tabBtns[cat] = t;
            tabBar.appendChild(t);
        });
        tabBtns[activeCat].style.background = 'var(--accent)';
        tabBtns[activeCat].style.color = '#fff';
        symbolPicker.appendChild(tabBar);

        // Grid
        const grid = document.createElement('div');
        grid.style.cssText = 'display:grid;grid-template-columns:repeat(8,1fr);gap:3px;overflow-y:auto;max-height:220px;';
        symbolPicker.appendChild(grid);

        function renderGrid(cat, filter) {
            grid.innerHTML = '';
            const entries = filter
                ? Object.values(SYMBOL_CATEGORIES).flat().filter(([,name]) => name.includes(filter.toLowerCase()))
                : SYMBOL_CATEGORIES[cat] || [];
            entries.forEach(([char, name]) => {
                const btn = document.createElement('button');
                btn.className = 'symbol-btn';
                btn.dataset.char = char;
                btn.title = name;
                btn.textContent = char;
                btn.style.cssText = 'padding:4px;border:1px solid var(--border-subtle);border-radius:3px;font-size:14px;cursor:pointer;background:var(--chrome-bg);color:var(--text-primary);aspect-ratio:1;';
                btn.addEventListener('mousedown', ev => {
                    ev.preventDefault();
                    editor.focus();
                    document.execCommand('insertText', false, char);
                    symbolPicker.hidden = true;
                    editor.dispatchEvent(new Event('input'));
                    pushUndoSnapshot();
                });
                grid.appendChild(btn);
            });
            if (!entries.length) {
                const msg = document.createElement('div');
                msg.textContent = 'No results';
                msg.style.cssText = 'grid-column:1/-1;text-align:center;font-size:11px;color:var(--text-hint);padding:8px;';
                grid.appendChild(msg);
            }
        }

        filterInput.addEventListener('input', () => {
            const q = filterInput.value.trim();
            renderGrid(activeCat, q || null);
        });

        renderGrid(activeCat);
    }

    // Initialize on first open
    const btnSymbol = document.getElementById('btn-insert-symbol');
    btnSymbol?.addEventListener('click', () => {
        initFullSymbolPicker();
        symbolPicker.hidden = !symbolPicker.hidden;
        if (!symbolPicker.hidden) {
            const rect = btnSymbol.getBoundingClientRect();
            symbolPicker.style.top  = (rect.bottom + 4) + 'px';
            symbolPicker.style.left = rect.left + 'px';
            document.getElementById('symbol-filter')?.focus();
        }
    });
})();

// ── Sprint 4 + Polish CSS injection ──────────────────────────────────────────
(function() {
    const s = document.createElement('style');
    s.id = 'sprint4-polish-css';
    s.textContent = `
        /* L07: Drag-over highlight */
        .drag-over { outline: 3px dashed var(--accent) !important; outline-offset: -6px; }

        /* L10: Outline current heading */
        #outline-list .outline-current > a,
        #outline-list .outline-current { color: var(--accent) !important; font-weight: 600; }

        /* M22: Table styles */
        .document-page table.ts-striped tbody tr:nth-child(even) td { background: var(--chrome-bg); }
        .document-page table.ts-bordered td,
        .document-page table.ts-bordered th { border: 1px solid var(--border) !important; }
        .document-page table.ts-header-blue thead tr { background: var(--accent) !important; color: #fff !important; }
        .document-page table.ts-header-blue thead th { color: #fff !important; border-color: transparent; }
        .document-page table.ts-compact td,
        .document-page table.ts-compact th { padding: 2px 4px !important; font-size: 10px; }
        .document-page table.ts-modern { border-collapse: separate; border-spacing: 0; border-radius: 8px; overflow: hidden; box-shadow: 0 2px 8px rgba(0,0,0,0.1); }
        .document-page table.ts-modern thead tr { background: var(--accent); color: #fff; }
        .document-page table.ts-modern td { border-bottom: 1px solid var(--border-subtle); }
        .document-page table.ts-grid-light td,
        .document-page table.ts-grid-light th { border: 1px solid rgba(0,0,0,0.08); }
        .document-page table.ts-dark-accent thead tr { background: var(--text-primary) !important; color: var(--bg-primary) !important; }

        /* H29: Symbol picker button hover */
        .symbol-btn:hover { background: var(--accent-light) !important; border-color: var(--accent) !important; }
    `;
    document.head.appendChild(s);
})();

// ==========================================================================
// Sprint 7: Paste Special (M03) & Shortcut Panel (M01)
// ==========================================================================

(function initSprint7() {
    // ── Paste Special (M03) ──
    const pasteScrim = document.getElementById('paste-special-scrim');
    const btnPasteOk = document.getElementById('btn-paste-ok');
    const btnPasteCancel = document.getElementById('btn-paste-cancel');
    let pendingPasteData = null;

    document.addEventListener('keydown', e => {
        // Ctrl+Shift+V for Paste Special
        if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'v') {
            e.preventDefault();
            let readPromise;
            if (window.__TAURI__ && window.__TAURI__.clipboard) {
                readPromise = window.__TAURI__.clipboard.readText();
            } else {
                readPromise = navigator.clipboard.readText();
            }
            readPromise.then(text => {
                pendingPasteData = text;
                pasteScrim.hidden = false;
            }).catch(err => {
                console.error("Failed to read clipboard", err);
            });
        }
    });

    btnPasteCancel?.addEventListener('click', () => {
        pasteScrim.hidden = true;
        pendingPasteData = null;
    });

    btnPasteOk?.addEventListener('click', () => {
        pasteScrim.hidden = true;
        if (!pendingPasteData) return;
        
        const type = document.querySelector('input[name="paste-type"]:checked')?.value || 'plain';
        editor.focus();
        
        if (type === 'plain') {
            document.execCommand('insertText', false, pendingPasteData);
        } else if (type === 'html') {
            // Default paste behavior for rich text - we trigger a standard paste event
            document.execCommand('paste');
        } else if (type === 'markdown') {
            if (window.__TAURI_INVOKE__) {
                const { invoke } = window.__TAURI__.tauri;
                invoke('md_to_html', { markdown: pendingPasteData }).then(html => {
                    document.execCommand('insertHTML', false, html);
                });
            }
        }
        pendingPasteData = null;
    });

    // ── Shortcut Panel (M01) ──
    const scScrim = document.getElementById('shortcuts-scrim');
    const scGrid = document.getElementById('shortcuts-grid');
    const btnScClose = document.getElementById('btn-shortcuts-close');

    const SHORTCUTS = [
        { cat: 'File', keys: 'Ctrl+S', desc: 'Save Document' },
        { cat: 'File', keys: 'Ctrl+O', desc: 'Open Document' },
        { cat: 'File', keys: 'Ctrl+N', desc: 'New Document' },
        { cat: 'File', keys: 'Ctrl+P', desc: 'Print / PDF Export' },
        { cat: 'Editing', keys: 'Ctrl+Z', desc: 'Undo' },
        { cat: 'Editing', keys: 'Ctrl+Y', desc: 'Redo' },
        { cat: 'Editing', keys: 'Ctrl+F', desc: 'Find' },
        { cat: 'Editing', keys: 'Ctrl+H', desc: 'Replace' },
        { cat: 'Formatting', keys: 'Ctrl+B', desc: 'Bold' },
        { cat: 'Formatting', keys: 'Ctrl+I', desc: 'Italic' },
        { cat: 'Formatting', keys: 'Ctrl+U', desc: 'Underline' },
        { cat: 'Formatting', keys: 'Ctrl+1..6', desc: 'Heading 1..6' },
        { cat: 'Formatting', keys: 'Ctrl+Shift+L', desc: 'Bullet List' },
        { cat: 'Formatting', keys: 'Ctrl+L/E/R/J', desc: 'Align Left/Center/Right/Justify' },
        { cat: 'Insert', keys: 'Ctrl+K', desc: 'Insert Link' },
        { cat: 'Insert', keys: 'Ctrl+Shift+V', desc: 'Paste Special' },
        { cat: 'Review', keys: 'Ctrl+Alt+M', desc: 'Insert Comment' },
        { cat: 'Navigation', keys: '?', desc: 'Show this shortcuts panel' }
    ];

    if (scGrid) {
        let currentCat = '';
        let html = '';
        SHORTCUTS.forEach(sc => {
            if (sc.cat !== currentCat) {
                html += `<div style="font-weight:bold; margin-top:12px; margin-bottom:4px; color:var(--accent); border-bottom:1px solid var(--border-subtle);">${sc.cat}</div>`;
                currentCat = sc.cat;
            }
            html += `<div style="display:flex; justify-content:space-between; padding:4px 0; font-size:12px; border-bottom:1px solid rgba(0,0,0,0.03);">
                <span>${sc.desc}</span>
                <span style="font-family:monospace; background:var(--border-subtle); padding:2px 6px; border-radius:4px; color:var(--text-secondary);">${sc.keys}</span>
            </div>`;
        });
        scGrid.innerHTML = html;
    }

    document.addEventListener('keydown', e => {
        // Show panel on '?' if no input is focused
        if (e.key === '?' && document.activeElement !== editor && !editor.contains(document.activeElement) && document.activeElement.tagName !== 'INPUT' && document.activeElement.tagName !== 'TEXTAREA') {
            e.preventDefault();
            if (scScrim) scScrim.hidden = false;
        }
    });

    btnScClose?.addEventListener('click', () => { if(scScrim) scScrim.hidden = true; });
})();

// ==========================================================================
// M04/M05: Horizontal Ruler
// ==========================================================================
(function initRuler() {
    const btnRuler = document.getElementById('btn-ruler');
    const docRuler = document.getElementById('doc-ruler');
    if (!docRuler || !btnRuler) return;

    btnRuler.addEventListener('click', () => {
        const active = btnRuler.classList.toggle('rbtn--active-toggle');
        docRuler.hidden = !active;
    });

    // M05: Configurable tab stops
    const rulerTabs = document.getElementById('ruler-tabs');
    const marks = document.getElementById('ruler-marks');

    marks?.addEventListener('click', e => {
        // Calculate position relative to ruler-handles container
        const handlesRect = document.getElementById('ruler-handles').getBoundingClientRect();
        let pos = e.clientX - handlesRect.left;
        
        // Add tab stop
        const ts = document.createElement('div');
        ts.className = 'ruler-tab-stop';
        ts.style.left = pos + 'px';
        rulerTabs.appendChild(ts);
        
        // Click to remove
        ts.addEventListener('click', (ev) => {
            ev.stopPropagation();
            ts.remove();
        });
    });

    // Draggable margins
    let draggingHandle = null;
    const handles = document.querySelectorAll('.ruler-handle');
    handles.forEach(h => {
        h.addEventListener('mousedown', e => {
            draggingHandle = h;
            e.stopPropagation();
        });
    });

    document.addEventListener('mousemove', e => {
        if (!draggingHandle) return;
        const handlesRect = document.getElementById('ruler-handles').getBoundingClientRect();
        let pos = e.clientX - handlesRect.left;
        pos = Math.max(0, Math.min(pos, handlesRect.width));
        
        if (draggingHandle.id === 'ruler-lmargin' || draggingHandle.id === 'ruler-indent') {
            draggingHandle.style.left = pos + 'px';
            if (draggingHandle.id === 'ruler-lmargin') {
                document.getElementById('ruler-indent').style.left = pos + 'px'; // Move indent with lmargin
            }
        } else if (draggingHandle.id === 'ruler-rmargin') {
            draggingHandle.style.right = (handlesRect.width - pos) + 'px';
            draggingHandle.style.left = 'auto'; // Reset left just in case
        }
    });

    document.addEventListener('mouseup', () => {
        if (draggingHandle) {
            // Apply margins to document-page
            const page = document.querySelector('.document-page');
            if (page) {
                const l = document.getElementById('ruler-lmargin').style.left;
                const r = document.getElementById('ruler-rmargin').style.right;
                if (l) page.style.paddingLeft = l;
                if (r) page.style.paddingRight = r;
            }
            draggingHandle = null;
        }
    });
})();

// ==========================================================================
// M18: Text Box
// ==========================================================================
(function initTextBoxes() {
    const btn = document.getElementById('btn-insert-textbox');
    btn?.addEventListener('click', () => {
        const box = document.createElement('div');
        box.className = 'doc-textbox';
        box.contentEditable = 'false'; 
        // Position relative to editor
        box.style.top = '100px';
        box.style.left = '100px';

        const handle = document.createElement('div');
        handle.className = 'doc-textbox-handle';
        box.appendChild(handle);

        const content = document.createElement('div');
        content.contentEditable = 'true';
        content.innerHTML = 'Type here...';
        box.appendChild(content);

        editor.appendChild(box);

        // Dragging logic
        let isDragging = false;
        let startX, startY, initialL, initialT;

        handle.addEventListener('mousedown', e => {
            isDragging = true;
            startX = e.clientX;
            startY = e.clientY;
            initialL = parseInt(box.style.left || 0, 10);
            initialT = parseInt(box.style.top || 0, 10);
            e.stopPropagation();
        });

        document.addEventListener('mousemove', e => {
            if (!isDragging) return;
            box.style.left = (initialL + e.clientX - startX) + 'px';
            box.style.top = (initialT + e.clientY - startY) + 'px';
        });

        document.addEventListener('mouseup', () => {
            if (isDragging) {
                isDragging = false;
                editor.dispatchEvent(new Event('input'));
            }
        });
        
        // Remove on backspace if empty
        content.addEventListener('keydown', e => {
            if (e.key === 'Backspace' && content.innerText.trim() === '') {
                box.remove();
                editor.dispatchEvent(new Event('input'));
            }
        });
    });
})();

// ==========================================================================
// M07 & M23: Cell Properties (Padding & Border)
// ==========================================================================
(function initCellProps() {
    const scrim = document.getElementById('cell-props-scrim');
    if (!scrim) return;

    window.showCellPropsDialog = function() {
        if (!typeof ctxTargetCell !== 'undefined' && ctxTargetCell) {
            const comp = window.getComputedStyle(ctxTargetCell);
            document.getElementById('cp-pad-t').value = parseInt(comp.paddingTop) || 0;
            document.getElementById('cp-pad-r').value = parseInt(comp.paddingRight) || 0;
            document.getElementById('cp-pad-b').value = parseInt(comp.paddingBottom) || 0;
            document.getElementById('cp-pad-l').value = parseInt(comp.paddingLeft) || 0;

            document.getElementById('cp-border-w').value = parseInt(comp.borderTopWidth) || 1;
            document.getElementById('cp-border-s').value = comp.borderTopStyle || 'solid';
        }
        scrim.hidden = false;
    };

    document.getElementById('btn-cp-cancel')?.addEventListener('click', () => {
        scrim.hidden = true;
    });

    document.getElementById('btn-cp-ok')?.addEventListener('click', () => {
        if (typeof ctxTargetCell === 'undefined' || !ctxTargetCell) return;
        const pt = document.getElementById('cp-pad-t').value;
        const pr = document.getElementById('cp-pad-r').value;
        const pb = document.getElementById('cp-pad-b').value;
        const pl = document.getElementById('cp-pad-l').value;

        const cells = document.querySelectorAll('td.selected, th.selected');
        const targets = cells.length > 0 ? Array.from(cells) : [ctxTargetCell];

        targets.forEach(c => {
            c.style.padding = `${pt}px ${pr}px ${pb}px ${pl}px`;
        });

        editor.dispatchEvent(new Event('input'));
        scrim.hidden = true;
    });

    document.getElementById('cp-apply-all')?.addEventListener('click', () => {
        if (typeof ctxTargetCell === 'undefined' || !ctxTargetCell) return;
        const w = document.getElementById('cp-border-w').value + 'px';
        const s = document.getElementById('cp-border-s').value;
        const c = document.getElementById('cp-border-c').value;
        const borderStr = `${w} ${s} ${c}`;

        const cells = document.querySelectorAll('td.selected, th.selected');
        const targets = cells.length > 0 ? Array.from(cells) : [ctxTargetCell];

        targets.forEach(cell => {
            cell.style.border = borderStr;
        });
        editor.dispatchEvent(new Event('input'));
    });
})();

// ==========================================================================
// M16: Watermark
// ==========================================================================
(function initWatermark() {
    const btn = document.getElementById('btn-watermark');
    const scrim = document.getElementById('watermark-scrim');
    if (!btn || !scrim) return;

    btn.addEventListener('click', () => {
        scrim.hidden = false;
    });

    const radios = document.querySelectorAll('input[name="wm-type"]');
    const opts = document.getElementById('wm-text-opts');
    radios.forEach(r => r.addEventListener('change', () => {
        if (document.querySelector('input[name="wm-type"]:checked').value === 'text') {
            opts.style.opacity = '1';
            opts.style.pointerEvents = 'auto';
        } else {
            opts.style.opacity = '0.5';
            opts.style.pointerEvents = 'none';
        }
    }));

    document.getElementById('btn-wm-cancel')?.addEventListener('click', () => {
        scrim.hidden = true;
    });

    document.getElementById('btn-wm-ok')?.addEventListener('click', () => {
        const type = document.querySelector('input[name="wm-type"]:checked').value;
        
        // Remove existing watermark
        const existing = editor.querySelector('.doc-watermark');
        if (existing) existing.remove();

        if (type === 'text') {
            const wm = document.createElement('div');
            wm.className = 'doc-watermark';
            wm.contentEditable = 'false';
            wm.textContent = document.getElementById('wm-text').value;
            wm.style.color = document.getElementById('wm-color').value;
            wm.style.transform = `translate(-50%, -50%) rotate(${document.getElementById('wm-angle').value}deg)`;
            editor.appendChild(wm);
        }
        
        editor.dispatchEvent(new Event('input'));
        scrim.hidden = true;
    });
})();

// ==========================================================================
// M08: RTL Text Direction
// ==========================================================================
(function initRTL() {
    const btnRTL = document.getElementById('btn-rtl');
    btnRTL?.addEventListener('click', e => {
        e.preventDefault();
        const sel = window.getSelection();
        if (!sel.rangeCount || !editor.contains(sel.anchorNode)) return;
        
        let block = sel.anchorNode;
        if (block.nodeType === 3) block = block.parentElement;
        const blockTags = ['P', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'LI', 'DIV', 'TD', 'TH'];
        while (block && block !== editor && !blockTags.includes(block.tagName)) {
            block = block.parentElement;
        }
        if (block && block !== editor) {
            if (block.getAttribute('dir') === 'rtl') {
                block.removeAttribute('dir');
                btnRTL.classList.remove('rbtn--active-toggle');
            } else {
                block.setAttribute('dir', 'rtl');
                btnRTL.classList.add('rbtn--active-toggle');
            }
            editor.dispatchEvent(new Event('input'));
        }
    });
})();

// ==========================================================================
// M09: Read Aloud
// ==========================================================================
(function initReadAloud() {
    const btn = document.getElementById('btn-read-aloud');
    if (!btn) return;

    let isReading = false;

    btn.addEventListener('click', () => {
        if (isReading) {
            window.speechSynthesis.cancel();
            isReading = false;
            btn.classList.remove('rbtn--active-toggle');
            return;
        }

        const text = window.getSelection().toString() || editor.innerText;
        if (!text.trim()) return;

        const utterance = new SpeechSynthesisUtterance(text);
        
        utterance.onend = () => {
            isReading = false;
            btn.classList.remove('rbtn--active-toggle');
        };

        utterance.onerror = () => {
            isReading = false;
            btn.classList.remove('rbtn--active-toggle');
        };

        isReading = true;
        btn.classList.add('rbtn--active-toggle');
        window.speechSynthesis.speak(utterance);
    });
})();

// ==========================================================================
// M10: Per-Paragraph Language
// ==========================================================================
(function initLanguageProps() {
    const scrim = document.getElementById('lang-props-scrim');
    if (!scrim) return;

    ctxWire('ctx-lang-props', () => {
        if (!ctxTargetParagraph) return;
        document.getElementById('lang-select').value = ctxTargetParagraph.getAttribute('lang') || '';
        scrim.hidden = false;
    });

    document.getElementById('btn-lang-cancel')?.addEventListener('click', () => {
        scrim.hidden = true;
    });

    document.getElementById('btn-lang-ok')?.addEventListener('click', () => {
        if (!ctxTargetParagraph) return;
        const val = document.getElementById('lang-select').value;
        if (val) {
            ctxTargetParagraph.setAttribute('lang', val);
        } else {
            ctxTargetParagraph.removeAttribute('lang');
        }
        editor.dispatchEvent(new Event('input'));
        scrim.hidden = true;
    });
})();

// ==========================================================================
// L01: Styled Shortcut Tooltips
// ==========================================================================
(function initTooltips() {
    const tooltip = document.createElement('div');
    tooltip.id = 'custom-tooltip';
    document.body.appendChild(tooltip);

    let activeEl = null;

    document.querySelectorAll('[title]').forEach(el => {
        const rawTitle = el.getAttribute('title');
        if (!rawTitle) return;

        // Parse "Command Name (Ctrl+X)"
        const match = rawTitle.match(/(.*?)\s*\((.*?)\)$/);
        if (match) {
            el.setAttribute('data-tooltip-name', match[1].trim());
            el.setAttribute('data-tooltip-key', match[2].trim());
        } else {
            el.setAttribute('data-tooltip-name', rawTitle.trim());
        }
        
        // Remove native title
        el.removeAttribute('title');

        el.addEventListener('mouseenter', e => {
            activeEl = el;
            const name = el.getAttribute('data-tooltip-name');
            const key = el.getAttribute('data-tooltip-key');
            
            if (key) {
                tooltip.innerHTML = `${name} <kbd>${key}</kbd>`;
            } else {
                tooltip.innerHTML = name;
            }

            const rect = el.getBoundingClientRect();
            // Position below the element
            tooltip.style.left = rect.left + 'px';
            tooltip.style.top = (rect.bottom + 6) + 'px';
            
            tooltip.style.opacity = '1';

            // Wait a tick to get true dimensions, then adjust if offscreen
            setTimeout(() => {
                if (activeEl === el) {
                    const ttRect = tooltip.getBoundingClientRect();
                    if (ttRect.right > window.innerWidth) {
                        tooltip.style.left = Math.max(4, window.innerWidth - ttRect.width - 4) + 'px';
                    }
                }
            }, 0);
        });

        el.addEventListener('mouseleave', () => {
            if (activeEl === el) {
                tooltip.style.opacity = '0';
                activeEl = null;
            }
        });
        
        // Also hide on click since it shouldn't persist
        el.addEventListener('mousedown', () => {
            tooltip.style.opacity = '0';
            activeEl = null;
        });
    });
})();

// ==========================================================================
// L09: Select-All in Table
// ==========================================================================
(function initSelectAll() {
    let lastSelectAllTime = 0;

    editor.addEventListener('keydown', e => {
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
            const sel = window.getSelection();
            if (!sel.rangeCount || !editor.contains(sel.anchorNode)) return;

            let node = sel.anchorNode;
            if (node.nodeType === 3) node = node.parentElement;
            const cell = node.closest('td, th');
            
            if (cell) {
                const table = cell.closest('table');
                
                // If they pressed it twice quickly, or all cells are already selected, let it select the whole doc
                if (Date.now() - lastSelectAllTime < 1000) {
                    return; // let default behavior happen
                }

                e.preventDefault();
                clearCellSelection();
                const cells = table.querySelectorAll('td, th');
                cells.forEach(c => {
                    c.classList.add('cell-selected');
                    selectedCells.add(c);
                });
                
                // Empty the native range to prevent it fighting our CSS selection
                sel.removeAllRanges();
                
                lastSelectAllTime = Date.now();
            }
        }
    });
})();

// ── Document Properties (G-M31) ───────────────────────────────────────────────
// Stores title/author/subject/keywords/date in designState for use by exporters.
(function() {
    const DOC_PROPS_KEY = 'marksmen-doc-props-v1';
    const scrim   = document.getElementById('doc-props-scrim');
    const fTitle  = document.getElementById('doc-prop-title');
    const fAuthor = document.getElementById('doc-prop-author');
    const fSubj   = document.getElementById('doc-prop-subject');
    const fKw     = document.getElementById('doc-prop-keywords');
    const fDate   = document.getElementById('doc-prop-date');
    if (!scrim || !fTitle) return;

    // Canonical property store; consumed by flush() when building frontmatter.
    if (!window.docProps) window.docProps = { title: '', author: '', subject: '', keywords: '', date: '' };

    function loadDocProps() {
        try {
            const stored = localStorage.getItem(DOC_PROPS_KEY);
            if (stored) Object.assign(window.docProps, JSON.parse(stored));
        } catch {}
    }
    function saveDocProps() {
        localStorage.setItem(DOC_PROPS_KEY, JSON.stringify(window.docProps));
    }
    function openDocProps() {
        loadDocProps();
        fTitle.value  = window.docProps.title;
        fAuthor.value = window.docProps.author;
        fSubj.value   = window.docProps.subject;
        fKw.value     = window.docProps.keywords;
        fDate.value   = window.docProps.date;
        scrim.hidden  = false;
        // Move focus into dialog so scrim keydown listener fires
        requestAnimationFrame(() => fTitle.focus());
    }
    function closeDocProps() {
        scrim.hidden = true;
    }
    function applyDocProps() {
        window.docProps.title    = fTitle.value.trim();
        window.docProps.author   = fAuthor.value.trim();
        window.docProps.subject  = fSubj.value.trim();
        window.docProps.keywords = fKw.value.trim();
        window.docProps.date     = fDate.value;
        saveDocProps();
        // Update window title to reflect doc title
        if (window.docProps.title) {
            document.title = `${window.docProps.title} \u2014 Marksmen Editor`;
        }
        closeDocProps();
    }

    document.getElementById('doc-props-save')?.addEventListener('click', applyDocProps);
    document.getElementById('doc-props-cancel')?.addEventListener('click', closeDocProps);
    document.getElementById('doc-props-close')?.addEventListener('click', closeDocProps);
    scrim.addEventListener('click', e => { if (e.target === scrim) closeDocProps(); });
    // Keyboard handling scoped to the dialog — prevents Enter/Escape from leaking
    // into the editor when the dialog is dismissed (G-M31 exit bug).
    scrim.addEventListener('keydown', e => {
        if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeDocProps(); }
        if (e.key === 'Enter' && document.activeElement !== document.getElementById('doc-props-cancel'))
            { e.preventDefault(); e.stopPropagation(); applyDocProps(); }
    });
    // Also allow Escape from the document level (handles focus outside the scrim)
    document.addEventListener('keydown', e => {
        if (!scrim.hidden && e.key === 'Escape') { e.preventDefault(); closeDocProps(); }
    });

    // Wire File menu "Properties" and status-bar word-count click
    document.getElementById('btn-doc-properties')?.addEventListener('click', openDocProps);
    document.getElementById('sbar-word-count')?.addEventListener('click', openDocProps);

    // Wire Ctrl+Shift+D as shortcut
    document.addEventListener('keydown', e => {
        if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'D') {
            e.preventDefault();
            openDocProps();
        }
    });

    loadDocProps();
})();

// ── Print Preview (G-M32) ─────────────────────────────────────────────────────
// Provides a summary dialog with page/word estimates before calling printDocument().
(function() {
    const scrim = document.getElementById('print-preview-scrim');
    if (!scrim) return;

    function openPrintPreview() {
        // Estimate page count: A4/Letter ~500 words/page at standard settings
        const text  = (editor.innerText || '').trim();
        const words = text ? text.split(/\s+/).length : 0;
        const pages = Math.max(1, Math.ceil(words / 500));
        const ppEl  = document.getElementById('pp-page-est');
        const wcEl  = document.getElementById('pp-word-count');
        if (ppEl) ppEl.textContent = pages;
        if (wcEl) wcEl.textContent = `${words.toLocaleString()} words`;
        scrim.hidden = false;
        // Move focus into dialog so keyboard handlers work immediately
        requestAnimationFrame(() => document.getElementById('pp-cancel-btn')?.focus());
    }
    function closePrintPreview() {
        scrim.hidden = true;
    }

    document.getElementById('print-preview-close')?.addEventListener('click', closePrintPreview);
    document.getElementById('pp-cancel-btn')?.addEventListener('click', closePrintPreview);
    scrim.addEventListener('click', e => { if (e.target === scrim) closePrintPreview(); });
    document.addEventListener('keydown', e => {
        if (!scrim.hidden && e.key === 'Escape') { e.preventDefault(); closePrintPreview(); }
    });

    document.getElementById('pp-print-btn')?.addEventListener('click', async () => {
        // Apply orientation to page before printing
        const orient = document.getElementById('pp-orientation')?.value;
        const grayMode = document.getElementById('pp-color-mode')?.value === 'gray';
        let styleTag = document.getElementById('pp-print-style');
        if (!styleTag) {
            styleTag = document.createElement('style');
            styleTag.id = 'pp-print-style';
            document.head.appendChild(styleTag);
        }
        styleTag.textContent = [
            '@media print {',
            orient === 'landscape' ? '@page { size: landscape; }' : '@page { size: portrait; }',
            grayMode ? '* { filter: grayscale(100%) !important; }' : '',
            '}'
        ].join('\n');

        closePrintPreview();
        await printDocument();
    });

    // Wire Ctrl+P to open preview instead of directly printing
    // (overrides the existing Ctrl+P handler in the main keydown block
    //  by replacing printDocument with openPrintPreview in a capture-phase handler)
    document.addEventListener('keydown', e => {
        if ((e.ctrlKey || e.metaKey) && e.key === 'p' && !e.shiftKey) {
            e.preventDefault();
            e.stopImmediatePropagation();
            openPrintPreview();
        }
    }, true); // capture phase — runs before the bubble-phase handler on line ~2140

    // Also wire the btn-print click to open the preview
    document.getElementById('btn-print')?.removeEventListener('click', () => window.print());
    document.getElementById('btn-print')?.addEventListener('click', openPrintPreview);
})();

// ── Version History (G-M28) ───────────────────────────────────────────────────
// Named snapshots stored in localStorage. Auto-snapshot on save; manual via File menu.
// Up to 50 versions retained; oldest evicted when limit exceeded.
(function() {
    const VH_KEY        = 'marksmen-versions-v1';
    const VH_MAX        = 50;
    const AUTO_INTERVAL = 5 * 60 * 1000; // 5 minutes

    const scrim    = document.getElementById('version-history-scrim');
    const list     = document.getElementById('vh-list');
    const emptyMsg = document.getElementById('vh-empty');
    if (!scrim || !list) return;

    // ── Storage ──────────────────────────────────────────────────────────────
    function loadVersions() {
        try { return JSON.parse(localStorage.getItem(VH_KEY) || '[]'); }
        catch { return []; }
    }
    function saveVersions(versions) {
        localStorage.setItem(VH_KEY, JSON.stringify(versions));
    }

    // ── Snapshot creation ────────────────────────────────────────────────────
    function createVersion(label) {
        const html     = editor.innerHTML;
        const markdown = typeof window.currentMarkdown === 'string' ? window.currentMarkdown : '';
        const versions = loadVersions();
        versions.unshift({
            id:       Date.now(),
            label:    label || new Date().toLocaleString(),
            html,
            markdown,
            words:    (editor.innerText || '').trim().split(/\s+/).filter(Boolean).length
        });
        if (versions.length > VH_MAX) versions.length = VH_MAX;
        saveVersions(versions);
        return versions;
    }

    // ── Render list ──────────────────────────────────────────────────────────
    function renderVersionList() {
        const versions = loadVersions();
        emptyMsg.hidden = versions.length > 0;
        list.innerHTML = '';
        versions.forEach((v, idx) => {
            const li = document.createElement('li');
            li.style.cssText = 'display:flex;align-items:center;gap:10px;padding:10px 16px;border-bottom:1px solid var(--border-color,#333);';
            li.innerHTML = `
                <div style="flex:1;min-width:0">
                    <div id="vhl-name-${v.id}" style="font-size:13px;font-weight:500;color:var(--text-primary);white-space:nowrap;overflow:hidden;text-overflow:ellipsis">${v.label}</div>
                    <div style="font-size:11px;color:var(--text-secondary);margin-top:2px">${v.words.toLocaleString()} words</div>
                </div>
                <button data-vh-rename="${idx}" class="modal-btn" style="font-size:11px;padding:3px 8px" title="Rename">✏️</button>
                <button data-vh-restore="${idx}" class="modal-btn modal-btn--primary" style="font-size:11px;padding:3px 8px">Restore</button>
                <button data-vh-delete="${idx}" class="modal-btn" style="font-size:11px;padding:3px 8px;color:#e55" title="Delete">✕</button>
            `;
            list.appendChild(li);
        });

        // Restore
        list.querySelectorAll('[data-vh-restore]').forEach(btn => {
            btn.addEventListener('click', () => {
                const versions = loadVersions();
                const v = versions[parseInt(btn.dataset.vhRestore, 10)];
                if (!v) return;
                if (!confirm(`Restore to "${v.label}"? Current content will be replaced.`)) return;
                editor.innerHTML = v.html;
                editor.dispatchEvent(new Event('input'));
                pushUndoSnapshot();
                closeVH();
                showToast('Restored to: ' + v.label);
            });
        });

        // Delete
        list.querySelectorAll('[data-vh-delete]').forEach(btn => {
            btn.addEventListener('click', () => {
                const versions = loadVersions();
                versions.splice(parseInt(btn.dataset.vhDelete, 10), 1);
                saveVersions(versions);
                renderVersionList();
            });
        });

        // Rename
        list.querySelectorAll('[data-vh-rename]').forEach(btn => {
            btn.addEventListener('click', () => {
                const versions = loadVersions();
                const idx = parseInt(btn.dataset.vhRename, 10);
                const v = versions[idx];
                const newName = prompt('Rename version:', v.label);
                if (newName !== null && newName.trim()) {
                    v.label = newName.trim();
                    saveVersions(versions);
                    renderVersionList();
                }
            });
        });
    }

    // ── Open / Close ─────────────────────────────────────────────────────────
    function openVH() {
        renderVersionList();
        scrim.hidden = false;
        requestAnimationFrame(() => document.getElementById('vh-save-version')?.focus());
    }
    function closeVH() { scrim.hidden = true; }

    document.getElementById('vh-close')?.addEventListener('click', closeVH);
    document.getElementById('vh-close-btn')?.addEventListener('click', closeVH);
    scrim.addEventListener('click', e => { if (e.target === scrim) closeVH(); });
    document.addEventListener('keydown', e => { if (!scrim.hidden && e.key === 'Escape') { e.preventDefault(); closeVH(); } });

    document.getElementById('vh-save-version')?.addEventListener('click', () => {
        const label = prompt('Version label:', new Date().toLocaleString());
        if (label !== null) {
            createVersion(label.trim() || new Date().toLocaleString());
            renderVersionList();
            showToast('Version saved');
        }
    });

    document.getElementById('vh-clear-all')?.addEventListener('click', () => {
        if (confirm('Delete all saved versions?')) {
            saveVersions([]);
            renderVersionList();
        }
    });

    // Wire entry points
    document.getElementById('btn-version-history')?.addEventListener('click', () => {
        // Close file menu first
        document.getElementById('file-menu')?.classList.remove('file-menu--open');
        document.getElementById('file-menu-scrim')?.setAttribute('hidden', '');
        openVH();
    });

    // Auto-snapshot hook: called by saveDocument()
    window.versionHistoryAutoSnapshot = function(label) {
        createVersion(label || ('Auto: ' + new Date().toLocaleString()));
    };

    // Periodic auto-snapshot every 5 minutes when document has content
    setInterval(() => {
        const text = (editor.innerText || '').trim();
        if (text.length > 50) createVersion('Auto: ' + new Date().toLocaleString());
    }, AUTO_INTERVAL);
})();

// ── Drop Cap (G-M17) ──────────────────────────────────────────────────────────
// Applies CSS ::first-letter drop cap to the paragraph containing the caret.
// Toggle on/off; data-drop-cap attribute on <p> drives the CSS.
(function() {
    const btn = document.getElementById('btn-drop-cap');
    if (!btn) return;

    // Inject drop-cap CSS once
    const styleEl = document.createElement('style');
    styleEl.textContent = `
        #editor [data-drop-cap]::first-letter {
            font-size: 3.8em;
            font-weight: 700;
            line-height: 0.8;
            float: left;
            margin: 0 6px 0 0;
            color: var(--accent, #7c6af7);
            font-family: Georgia, 'Times New Roman', serif;
        }
    `;
    document.head.appendChild(styleEl);

    function getCaretParagraph() {
        const sel = window.getSelection();
        if (!sel.rangeCount) return null;
        let node = sel.getRangeAt(0).startContainer;
        while (node && node !== editor) {
            if (node.nodeType === 1 && (node.tagName === 'P' || /^H[1-6]$/.test(node.tagName))) return node;
            node = node.parentElement;
        }
        return null;
    }

    btn.addEventListener('click', () => {
        const para = getCaretParagraph();
        if (!para) { showToast('Place cursor in a paragraph to apply Drop Cap'); return; }
        const isActive = para.hasAttribute('data-drop-cap');
        if (isActive) {
            para.removeAttribute('data-drop-cap');
        } else {
            para.setAttribute('data-drop-cap', '');
        }
        btn.classList.toggle('rbtn--active-toggle', para.hasAttribute('data-drop-cap'));
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
    });

    // Sync button state on caret move
    document.addEventListener('selectionchange', () => {
        const para = getCaretParagraph();
        btn.classList.toggle('rbtn--active-toggle', !!(para && para.hasAttribute('data-drop-cap')));
    });
})();

// ── Outline View (G-V08) ─────────────────────────────────────────────────────
// When view mode = 'outline', shows a floating panel listing all headings.
// Promote/Demote change heading level; clicking a heading scrolls to it.
(function() {
    const toolbar    = document.getElementById('outline-toolbar');
    const itemsList  = document.getElementById('outline-items');
    if (!toolbar || !itemsList) return;

    let selectedHeading = null;

    // ── Register 'outline' as a view mode ──
    // Patch VIEW_BODY_CLASSES at runtime so setViewMode('outline') adds 'view-outline'
    // The existing IIFE exposes setViewMode only locally; we intercept via the button click.
    const outlineBtn = document.getElementById('btn-view-outline');
    if (outlineBtn) {
        outlineBtn.addEventListener('click', () => {
            // Remove all view body classes added by the main setViewMode IIFE
            ['view-read', 'view-web', 'view-draft'].forEach(c => document.body.classList.remove(c));
            // Toggle outline
            const isOutline = document.body.classList.toggle('view-outline');
            outlineBtn.classList.toggle('rbtn--active-toggle', isOutline);
            document.querySelectorAll('.view-mode-btn:not(#btn-view-outline)').forEach(b => b.classList.remove('rbtn--active-toggle'));
            // Re-enable editing (was disabled in read mode)
            editor.contentEditable = 'true';
            if (isOutline) {
                toolbar.hidden = false;
                buildOutlineItems();
            } else {
                toolbar.hidden = true;
                selectedHeading = null;
            }
        });
    }

    function buildOutlineItems() {
        const headings = [...editor.querySelectorAll('h1,h2,h3,h4,h5,h6')]
            .filter(h => !h.classList.contains('page-break-ruler'));
        itemsList.innerHTML = '';
        if (headings.length === 0) {
            itemsList.innerHTML = '<li style="padding:16px;color:var(--text-secondary);font-size:13px;text-align:center">No headings found in document.</li>';
            return;
        }
        headings.forEach(h => {
            const level = parseInt(h.tagName[1], 10);
            const li = document.createElement('li');
            li.style.cssText = `padding:6px 16px 6px ${(level - 1) * 20 + 16}px;cursor:pointer;font-size:${14 - level}px;
                font-weight:${level <= 2 ? 600 : 400};border-bottom:1px solid var(--border-color,#222);
                transition:background 0.1s;color:var(--text-primary)`;
            li.textContent = h.textContent.trim() || `(${h.tagName})`;
            li.dataset.tag = h.tagName;
            li.addEventListener('click', () => {
                selectedHeading = h;
                h.scrollIntoView({ behavior: 'smooth', block: 'center' });
                itemsList.querySelectorAll('li').forEach(l => l.style.background = '');
                li.style.background = 'var(--accent-light, rgba(124,106,247,0.15))';
            });
            li.addEventListener('mouseenter', () => { li.style.background = 'var(--hover-bg, #2a2a3c)'; });
            li.addEventListener('mouseleave', () => {
                if (selectedHeading !== h) li.style.background = '';
            });
            itemsList.appendChild(li);
        });
    }

    // Promote: H2 → H1, H3 → H2 … (Tab in outline mode)
    function promoteHeading(h) {
        if (!h) { showToast('Click a heading to select it first'); return; }
        const level = parseInt(h.tagName[1], 10);
        if (level <= 1) return;
        const newTag = 'H' + (level - 1);
        const newH = document.createElement(newTag);
        newH.innerHTML = h.innerHTML;
        [...h.attributes].forEach(a => newH.setAttribute(a.name, a.value));
        h.replaceWith(newH);
        selectedHeading = newH;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
        buildOutlineItems();
    }

    // Demote: H1 → H2, H2 → H3 …
    function demoteHeading(h) {
        if (!h) { showToast('Click a heading to select it first'); return; }
        const level = parseInt(h.tagName[1], 10);
        if (level >= 6) return;
        const newTag = 'H' + (level + 1);
        const newH = document.createElement(newTag);
        newH.innerHTML = h.innerHTML;
        [...h.attributes].forEach(a => newH.setAttribute(a.name, a.value));
        h.replaceWith(newH);
        selectedHeading = newH;
        editor.dispatchEvent(new Event('input'));
        pushUndoSnapshot();
        buildOutlineItems();
    }

    document.getElementById('outline-promote')?.addEventListener('click', () => promoteHeading(selectedHeading));
    document.getElementById('outline-demote')?.addEventListener('click',  () => demoteHeading(selectedHeading));
    document.getElementById('outline-close-btn')?.addEventListener('click', () => {
        toolbar.hidden = true;
        document.body.classList.remove('view-outline');
        outlineBtn?.classList.remove('rbtn--active-toggle');
        selectedHeading = null;
    });

    // Keyboard shortcuts inside outline toolbar
    toolbar.addEventListener('keydown', e => {
        if (e.key === 'Escape') { document.getElementById('outline-close-btn')?.click(); }
        if (e.shiftKey && e.key === 'Tab') { e.preventDefault(); promoteHeading(selectedHeading); }
        if (!e.shiftKey && e.key === 'Tab') { e.preventDefault(); demoteHeading(selectedHeading); }
    });

    // Rebuild outline when editor content changes while in outline mode
    editor.addEventListener('input', () => {
        if (!toolbar.hidden) buildOutlineItems();
    });
})();

// ── Cover Page Gallery (G-H28) ────────────────────────────────────────────────
// Pre-built HTML cover page templates injected at the start of the editor.
(function() {
    const scrim  = document.getElementById('cover-page-scrim');
    const gallery = document.getElementById('cp-gallery');
    if (!scrim || !gallery) return;

    // ── Template definitions ────────────────────────────────────────────────
    // Each template is a self-contained HTML fragment inserted as-is.
    // Placeholders wrapped in [brackets] for the user to replace.
    const COVER_TEMPLATES = [
        {
            name: 'Classic',
            preview: '#2c3e50',
            html: `<div style="text-align:center;padding:80px 60px;background:#2c3e50;color:#fff;border-radius:4px;margin-bottom:32px;page-break-after:always">
<p style="font-size:11px;letter-spacing:4px;text-transform:uppercase;opacity:0.6;margin-bottom:48px">[ORGANIZATION]</p>
<h1 style="font-size:2.8em;font-weight:700;margin:0 0 16px;line-height:1.1">[Document Title]</h1>
<p style="font-size:1.2em;opacity:0.75;margin:0 0 64px">[Subtitle or description]</p>
<hr style="border:none;border-top:1px solid rgba(255,255,255,0.3);margin:0 auto;width:60px">
<p style="font-size:12px;opacity:0.5;margin-top:24px">[Author Name] · [Date]</p>
</div>`
        },
        {
            name: 'Modern',
            preview: '#7c6af7',
            html: `<div style="display:flex;min-height:240px;background:linear-gradient(135deg,#7c6af7 0%,#a78bfa 100%);color:#fff;border-radius:4px;overflow:hidden;margin-bottom:32px;page-break-after:always">
<div style="flex:0 0 8px;background:rgba(255,255,255,0.2)"></div>
<div style="flex:1;padding:60px 48px;display:flex;flex-direction:column;justify-content:center">
<p style="font-size:10px;letter-spacing:3px;text-transform:uppercase;opacity:0.7;margin:0 0 20px">[CATEGORY]</p>
<h1 style="font-size:2.4em;font-weight:800;margin:0 0 12px;line-height:1.05">[Document Title]</h1>
<p style="font-size:1em;opacity:0.8;margin:0 0 40px">[Subtitle or brief description of this document]</p>
<p style="font-size:11px;opacity:0.6;margin:0">[Author] · [Date]</p>
</div>
</div>`
        },
        {
            name: 'Minimal',
            preview: '#f8f9fa',
            html: `<div style="padding:80px 60px 60px;border-left:4px solid #333;margin-bottom:32px;page-break-after:always">
<p style="font-size:10px;letter-spacing:3px;text-transform:uppercase;color:#888;margin:0 0 32px">[DOCUMENT TYPE]</p>
<h1 style="font-size:3em;font-weight:300;margin:0 0 8px;color:#222;line-height:1.1">[Document Title]</h1>
<p style="font-size:1.1em;color:#555;margin:0 0 64px">[Subtitle]</p>
<p style="font-size:12px;color:#999;margin:0">[Author Name] · [Date]</p>
</div>`
        },
        {
            name: 'Executive',
            preview: '#1a1a2e',
            html: `<div style="text-align:center;padding:100px 60px;background:#1a1a2e;color:#fff;border-radius:4px;margin-bottom:32px;position:relative;overflow:hidden;page-break-after:always">
<div style="position:absolute;top:0;left:0;right:0;height:4px;background:linear-gradient(90deg,#e8b931,#f5d76e)"></div>
<p style="font-size:10px;letter-spacing:4px;text-transform:uppercase;color:#e8b931;margin:0 0 56px">[CONFIDENTIAL / INTERNAL]</p>
<h1 style="font-size:2.6em;font-weight:600;margin:0 0 16px;line-height:1.1">[Document Title]</h1>
<p style="font-size:1em;color:rgba(255,255,255,0.6);margin:0 0 80px">[Executive summary or purpose statement]</p>
<div style="display:flex;justify-content:center;gap:48px">
<div><p style="font-size:10px;color:#e8b931;margin:0 0 4px;text-transform:uppercase;letter-spacing:2px">Prepared By</p><p style="font-size:13px;margin:0">[Author Name]</p></div>
<div><p style="font-size:10px;color:#e8b931;margin:0 0 4px;text-transform:uppercase;letter-spacing:2px">Date</p><p style="font-size:13px;margin:0">[Date]</p></div>
</div>
</div>`
        },
    ];

    // ── Build gallery cards ─────────────────────────────────────────────────
    COVER_TEMPLATES.forEach((tmpl, idx) => {
        const card = document.createElement('div');
        card.style.cssText = `
            cursor:pointer;border-radius:8px;overflow:hidden;border:2px solid var(--border);
            transition:border-color 0.15s,transform 0.15s;display:flex;flex-direction:column;
        `;
        card.innerHTML = `
            <div style="height:100px;background:${tmpl.preview};"></div>
            <div style="padding:8px 10px;font-size:12px;font-weight:500;color:var(--text-primary);background:var(--chrome-bg)">${tmpl.name}</div>
        `;
        card.addEventListener('mouseenter', () => { card.style.borderColor = 'var(--accent)'; card.style.transform = 'scale(1.03)'; });
        card.addEventListener('mouseleave', () => { card.style.borderColor = 'var(--border)'; card.style.transform = ''; });
        card.addEventListener('click', () => {
            pushUndoSnapshot();
            // Insert cover HTML at the start of editor
            const temp = document.createElement('div');
            temp.innerHTML = tmpl.html;
            const coverNode = temp.firstElementChild;
            editor.insertBefore(coverNode, editor.firstChild);
            editor.dispatchEvent(new Event('input'));
            closeCoverPage();
            showToast(`Cover page "${tmpl.name}" inserted`);
        });
        gallery.appendChild(card);
    });

    function openCoverPage() {
        scrim.hidden = false;
        requestAnimationFrame(() => document.getElementById('cp-close-btn')?.focus());
    }
    function closeCoverPage() { scrim.hidden = true; }

    document.getElementById('cp-close')?.addEventListener('click', closeCoverPage);
    document.getElementById('cp-close-btn')?.addEventListener('click', closeCoverPage);
    scrim.addEventListener('click', e => { if (e.target === scrim) closeCoverPage(); });
    document.addEventListener('keydown', e => { if (!scrim.hidden && e.key === 'Escape') { e.preventDefault(); closeCoverPage(); } });

    document.getElementById('btn-insert-cover')?.addEventListener('click', openCoverPage);
})();

// ── Page Borders Dialog (G-D08 upgrade) ───────────────────────────────────────
// Full border dialog: style, color, width, scope (all / first-page).
// Replaces the trivial toggle in the Design panel IIFE.
(function() {
    const scrim   = document.getElementById('page-borders-scrim');
    const preview = document.getElementById('pb-preview');
    if (!scrim || !preview) return;

    const styleEl = document.createElement('style');
    styleEl.id = 'page-borders-style';
    document.head.appendChild(styleEl);

    // ── Live preview ──────────────────────────────────────────────────────
    function getValues() {
        return {
            style: document.getElementById('pb-style')?.value || 'solid',
            color: document.getElementById('pb-color')?.value || '#333333',
            width: document.getElementById('pb-width')?.value || '2',
            scope: document.getElementById('pb-scope')?.value || 'all',
        };
    }

    function updatePreview() {
        const v = getValues();
        if (v.style === 'none') {
            preview.style.border = '2px solid var(--border)';
        } else {
            preview.style.border = `${v.width}px ${v.style} ${v.color}`;
        }
    }

    ['pb-style', 'pb-color', 'pb-width', 'pb-scope'].forEach(id => {
        document.getElementById(id)?.addEventListener('input', updatePreview);
    });
    document.getElementById('pb-width')?.addEventListener('input', e => {
        document.getElementById('pb-width-label').textContent = e.target.value + 'px';
    });

    // ── Apply ─────────────────────────────────────────────────────────────
    function applyBorders() {
        const v = getValues();
        const pages = document.querySelectorAll('.document-page');
        if (v.style === 'none') {
            styleEl.textContent = '';
            pages.forEach(p => p.style.removeProperty('outline'));
        } else {
            const borderCSS = `${v.width}px ${v.style} ${v.color}`;
            if (v.scope === 'all') {
                styleEl.textContent = `.document-page { outline: ${borderCSS} !important; }`;
            } else {
                // first-page: target only the first .document-page
                styleEl.textContent = `.document-page:first-of-type { outline: ${borderCSS} !important; }`;
            }
        }
        showToast(v.style === 'none' ? 'Page borders removed' : 'Page borders applied');
    }

    document.getElementById('pb-apply')?.addEventListener('click', () => { applyBorders(); closePB(); });
    document.getElementById('pb-remove')?.addEventListener('click', () => {
        document.getElementById('pb-style').value = 'none';
        applyBorders();
        closePB();
    });

    function openPB() {
        updatePreview();
        scrim.hidden = false;
        requestAnimationFrame(() => document.getElementById('pb-style')?.focus());
    }
    function closePB() { scrim.hidden = true; }

    document.getElementById('pb-close')?.addEventListener('click', closePB);
    document.getElementById('pb-close-btn')?.addEventListener('click', closePB);
    scrim.addEventListener('click', e => { if (e.target === scrim) closePB(); });
    document.addEventListener('keydown', e => { if (!scrim.hidden && e.key === 'Escape') { e.preventDefault(); closePB(); } });

    // Override the trivial Design-panel toggle from earlier IIFE
    document.getElementById('btn-design-page-borders')?.addEventListener('click', openPB);
})();

// ── Ruler Tick Labels (G-M04 completion) ──────────────────────────────────────
// Draws inch number labels (1, 2, 3…) on the ruler marks bar.
// Called after initRuler() has run; appends label spans to .ruler-marks.
(function() {
    const marks = document.getElementById('ruler-marks');
    if (!marks) return;

    // 96px = 1 inch at standard screen DPI. Ruler width = 816px = 8.5 inches.
    // Left margin triangle sits at 96px, so labels start at 1".
    const INCH_PX = 96;
    const TOTAL_INCHES = 8; // 1..8 labelled (margins eat ~0.5" each side)

    for (let i = 1; i <= TOTAL_INCHES; i++) {
        const label = document.createElement('span');
        label.textContent = String(i);
        label.style.cssText = `
            position: absolute;
            left: ${i * INCH_PX - 4}px;
            bottom: 2px;
            font-size: 8px;
            color: var(--text-hint, #aaa);
            line-height: 1;
            pointer-events: none;
            user-select: none;
        `;
        marks.appendChild(label);
    }
})();

// ── AI Assistant Sidebar ──────────────────────────────────────────────────────
(function initAiSidebar() {
    // DOM handles
    const stabAi        = document.getElementById('stab-ai');
    const aiPanel       = document.getElementById('ai-panel');
    const aiConnectBtn  = document.getElementById('ai-connect');
    const aiHostInput   = document.getElementById('ai-host');
    const aiModelSelect = document.getElementById('ai-model-select');
    const aiStatusDot   = document.getElementById('ai-status-dot');
    const aiUserInput   = document.getElementById('ai-user-input');
    const aiSendBtn     = document.getElementById('ai-send');
    const aiResponse    = document.getElementById('ai-response');
    const aiThinking    = document.getElementById('ai-thinking');
    const aiApplyRow    = document.getElementById('ai-apply-row');
    const aiApplyReplace = document.getElementById('ai-apply-replace');
    const aiApplyInsert  = document.getElementById('ai-apply-insert');
    const aiCopyResp    = document.getElementById('ai-copy-resp');
    const aiSystemPrompt = document.getElementById('ai-system-prompt');
    const aiUseSelection = document.getElementById('ai-use-selection');

    if (!stabAi) return; // Guard: HTML not present

    // ── Tab wiring ─────────────────────────────────────────────────
    stabAi.addEventListener('click', () => {
        // Extend existing switchSidebarTab to handle 'ai'
        document.getElementById('stab-comments').classList.remove('stab--active');
        document.getElementById('stab-outline').classList.remove('stab--active');
        stabAi.classList.add('stab--active');

        document.getElementById('comments-list').hidden = true;
        document.getElementById('outline-list').hidden  = true;
        aiPanel.hidden = false;

        // Open the sidebar if collapsed
        const sb = document.getElementById('sidebar');
        if (sb.classList.contains('collapsed')) sb.classList.remove('collapsed');
    });

    // Patch existing switchSidebarTab to also deactivate the AI tab
    const origSwitchTab = window.switchSidebarTab;
    if (typeof origSwitchTab === 'function') {
        window.switchSidebarTab = function(tab) {
            stabAi.classList.remove('stab--active');
            aiPanel.hidden = true;
            origSwitchTab(tab);
        };
    }

    // ── State ──────────────────────────────────────────────────────
    let lastResponse    = '';
    let savedSelection  = null; // Saved Range before send so we can apply later
    let abortController = null;

    function setDot(state) {
        aiStatusDot.className = 'ai-dot ai-dot--' + state;
        const titles = { off: 'Disconnected', connecting: 'Connecting…', on: 'Connected', error: 'Error' };
        aiStatusDot.title = titles[state] || state;
    }

    // ── Connect — fetch /v1/models ──────────────────────────────────
    aiConnectBtn.addEventListener('click', async () => {
        const host = aiHostInput.value.trim().replace(/\/$/, '');
        setDot('connecting');
        aiConnectBtn.disabled = true;
        aiConnectBtn.textContent = '…';
        try {
            const res = await fetch(`${host}/v1/models`, {
                signal: AbortSignal.timeout(6000),
            });
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            const data = await res.json();
            const models = (data.data || []).map(m => m.id).filter(Boolean);
            if (models.length === 0) throw new Error('No models returned');

            // Populate select
            aiModelSelect.innerHTML = '';
            models.forEach(id => {
                const opt = document.createElement('option');
                opt.value = id;
                opt.textContent = id;
                aiModelSelect.appendChild(opt);
            });
            aiModelSelect.disabled = false;
            setDot('on');
            
            const savedModel = localStorage.getItem('marksmen-ai-model');
            if (savedModel && models.includes(savedModel)) {
                aiModelSelect.value = savedModel;
            }
            localStorage.setItem('marksmen-ai-host', host);
        } catch (err) {
            setDot('error');
            aiModelSelect.innerHTML = '<option value="">— connection failed —</option>';
            aiModelSelect.disabled = true;
            console.error('[AI] Connect failed:', err);
        } finally {
            aiConnectBtn.disabled = false;
            aiConnectBtn.textContent = '⟳ Connect';
        }
    });

    aiModelSelect.addEventListener('change', () => {
        localStorage.setItem('marksmen-ai-model', aiModelSelect.value);
    });

    // ── Preset buttons ──────────────────────────────────────────────
    document.querySelectorAll('.ai-preset').forEach(btn => {
        btn.addEventListener('click', () => {
            const prompt = btn.dataset.prompt || '';
            aiUserInput.value = prompt;
            aiUserInput.focus();
            aiUserInput.dispatchEvent(new Event('input'));
        });
    });

    // ── Send ────────────────────────────────────────────────────────
    aiUserInput.addEventListener('keydown', e => {
        if (e.key === 'Enter' && e.shiftKey) {
            e.preventDefault();
            sendToAI();
        }
    });
    aiSendBtn.addEventListener('click', sendToAI);

    async function sendToAI() {
        const host  = aiHostInput.value.trim().replace(/\/$/, '');
        const model = aiModelSelect.value;
        const userText = aiUserInput.value.trim();
        if (!model || !userText) return;

        // Save current selection so apply actions can use it later
        const sel = window.getSelection();
        if (sel && sel.rangeCount > 0) {
            savedSelection = sel.getRangeAt(0).cloneRange();
        } else {
            savedSelection = null;
        }

        // Build context from selection if requested
        let contextBlock = '';
        const aiUseDocument = document.getElementById('ai-use-document');
        const aiUseReferences = document.getElementById('ai-use-references');

        if (aiUseDocument && aiUseDocument.checked) {
            const docText = document.getElementById('editor').innerText.trim();
            if (docText) {
                contextBlock += '\n\n=== FULL DOCUMENT TEXT ===\n' + docText + '\n==========================\n';
            }
        } else if (aiUseSelection.checked && savedSelection) {
            const selText = savedSelection.toString().trim();
            if (selText) {
                contextBlock += '\n\n=== SELECTED TEXT ===\n' + selText + '\n=====================\n';
            }
        }

        if (aiUseReferences && aiUseReferences.checked) {
            const sources = window.marksmenSources || [];
            if (sources.length > 0) {
                const refsText = sources.map(s => {
                    let r = `[ID: ${s.id}] `;
                    if (s.author) r += s.author + " ";
                    if (s.year) r += `(${s.year}). `;
                    if (s.title) r += `"${s.title}". `;
                    if (s.publisher) r += `${s.publisher}. `;
                    if (s.url) r += `URL: ${s.url}`;
                    return r.trim();
                }).join('\n');
                contextBlock += '\n\n=== AVAILABLE REFERENCES LIBRARY ===\n' + refsText + '\n(Note: Pay attention to which of these have already been cited in the text and which have not.)\n====================================\n';
            }
        }

        const messages = [
            { role: 'system', content: aiSystemPrompt.value.trim() || 'You are a helpful writing assistant.' },
            { role: 'user',   content: userText + contextBlock },
        ];

        // UI: start thinking
        if (abortController) abortController.abort();
        abortController = new AbortController();
        aiThinking.hidden = false;
        aiApplyRow.hidden = true;
        aiSendBtn.disabled = true;
        lastResponse = '';

        const emptyMsg = aiResponse.querySelector('.ai-response-empty');
        if (emptyMsg) emptyMsg.remove();

        function escapeHtml(str) {
            return str.replace(/[&<>"']/g, m => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;' }[m]));
        }

        const userBubble = document.createElement('div');
        userBubble.className = 'ai-chat-msg';
        userBubble.style.marginBottom = '8px';
        userBubble.innerHTML = `<div class="ai-role-badge ai-role-badge--user">User</div><div style="white-space: pre-wrap; font-size: 11px; opacity: 0.9;">${escapeHtml(userText)}</div>`;
        aiResponse.appendChild(userBubble);

        const aiBubble = document.createElement('div');
        aiBubble.className = 'ai-chat-msg';
        aiBubble.style.marginBottom = '12px';
        aiBubble.style.paddingBottom = '8px';
        aiBubble.style.borderBottom = '1px solid var(--border)';
        aiBubble.innerHTML = `<div class="ai-role-badge ai-role-badge--assistant">Assistant</div><div class="ai-msg-content" style="white-space: pre-wrap;"></div>`;
        aiResponse.appendChild(aiBubble);
        const aiContentDiv = aiBubble.querySelector('.ai-msg-content');

        aiResponse.scrollTop = aiResponse.scrollHeight;

        try {
            const res = await fetch(`${host}/v1/chat/completions`, {
                method:  'POST',
                headers: { 'Content-Type': 'application/json' },
                body:    JSON.stringify({ model, messages, stream: true, temperature: 0.7 }),
                signal:  abortController.signal,
            });

            if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`);

            // Streaming SSE response
            const reader  = res.body.getReader();
            const decoder = new TextDecoder();
            let buffer    = '';

            aiThinking.hidden = true;

            while (true) {
                const { done, value } = await reader.read();
                if (done) break;
                buffer += decoder.decode(value, { stream: true });

                // Process complete SSE lines
                const lines = buffer.split('\n');
                buffer = lines.pop(); // keep partial line

                for (const line of lines) {
                    if (!line.startsWith('data:')) continue;
                    const data = line.slice(5).trim();
                    if (data === '[DONE]') break;
                    try {
                        const chunk = JSON.parse(data);
                        const delta = chunk.choices?.[0]?.delta?.content || '';
                        if (delta) {
                            lastResponse += delta;
                            aiContentDiv.textContent = lastResponse;
                            aiResponse.scrollTop   = aiResponse.scrollHeight;
                        }
                    } catch { /* partial JSON — ignore */ }
                }
            }

            // Show apply row when we have content
            if (lastResponse.trim()) {
                aiApplyRow.hidden = false;
                setDot('on');
            }
        } catch (err) {
            if (err.name === 'AbortError') {
                aiContentDiv.textContent = '(cancelled)';
            } else {
                aiContentDiv.textContent = `Error: ${err.message}`;
                setDot('error');
                console.error('[AI] Send failed:', err);
            }
        } finally {
            aiThinking.hidden = true;
            aiSendBtn.disabled = false;
            saveAiState();
        }
    }

    // ── Apply: Replace selection ────────────────────────────────────
    aiApplyReplace.addEventListener('click', () => {
        if (!lastResponse || !savedSelection) return;
        const sel = window.getSelection();
        sel.removeAllRanges();
        sel.addRange(savedSelection);
        if (!sel.isCollapsed) {
            document.execCommand('insertText', false, lastResponse.trim());
        }
    });

    // ── Apply: Insert after cursor ─────────────────────────────────
    aiApplyInsert.addEventListener('click', () => {
        if (!lastResponse) return;
        const editorEl = document.getElementById('editor');
        editorEl.focus();
        const sel = window.getSelection();
        if (sel && sel.rangeCount > 0) {
            const range = sel.getRangeAt(0);
            range.collapse(false); // move to end of selection
            document.execCommand('insertText', false, '\n' + lastResponse.trim());
        }
    });

    // ── Apply: Copy ────────────────────────────────────────────────
    aiCopyResp.addEventListener('click', () => {
        if (!lastResponse) return;
        navigator.clipboard.writeText(lastResponse.trim()).then(() => {
            const orig = aiCopyResp.textContent;
            aiCopyResp.textContent = '✓ Copied';
            setTimeout(() => { aiCopyResp.textContent = orig; }, 1500);
        });
    });

    // ── Persistence functions ─────────────────────────────────────────
    function saveAiState() {
        localStorage.setItem('marksmen-ai-history', aiResponse.innerHTML);
        localStorage.setItem('marksmen-ai-last-resp', lastResponse);
    }

    function loadAiState() {
        const history = localStorage.getItem('marksmen-ai-history');
        if (history) {
            aiResponse.innerHTML = history;
            aiResponse.scrollTop = aiResponse.scrollHeight;
        }
        lastResponse = localStorage.getItem('marksmen-ai-last-resp') || '';
        if (lastResponse.trim()) {
            aiApplyRow.hidden = false;
        }

        const savedHost = localStorage.getItem('marksmen-ai-host');
        if (savedHost) {
            aiHostInput.value = savedHost;
            setTimeout(() => aiConnectBtn.click(), 50); // Auto-connect
        }
    }

    const aiClearBtn = document.getElementById('ai-clear-chat');
    if (aiClearBtn) {
        aiClearBtn.addEventListener('click', () => {
            aiResponse.innerHTML = '<div class="ai-response-empty">Responses will appear here.</div>';
            lastResponse = '';
            aiApplyRow.hidden = true;
            saveAiState();
        });
    }

    loadAiState();
})();
