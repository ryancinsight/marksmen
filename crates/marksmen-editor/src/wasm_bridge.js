/**
 * wasm_bridge.js
 *
 * Unified IPC bridge for Marksmen frontend.
 *
 * Routing logic:
 *   - In Tauri desktop: forwards every call to window.__TAURI__.core.invoke.
 *   - In browser (static dev server / debug): routes to the WASM module where
 *     possible, or provides a best-effort browser-native fallback.
 *
 * All commands exposed by src-tauri/src/lib.rs are handled here so that the
 * editor, references, and mailings modules remain environment-agnostic.
 */

import init, { md_to_html, html_to_md, export_document, format_csl_citation, format_csl_bibliography, execute_mail_merge as wasm_execute_mail_merge } from './wasm/marksmen_wasm.js';

let wasmReady = false;

// Initialize WASM eagerly; callers that need it gate on the promise.
const wasmInit = init().then(() => { wasmReady = true; });

// ── Browser-mode state ────────────────────────────────────────────────────────
const LOCAL_SOURCES_KEY  = 'marksmen_cite_sources';
const LOCAL_AUTOSAVE_KEY = 'marksmen_autosave';

// ── Primary export ────────────────────────────────────────────────────────────
export async function invoke(cmd, args = {}) {
    // ── Tauri path ──────────────────────────────────────────────────────────
    if (window.__TAURI__) {
        return window.__TAURI__.core.invoke(cmd, args);
    }

    // ── WASM / browser path ─────────────────────────────────────────────────
    // Ensure WASM is initialised before dispatching commands that need it.
    if (!wasmReady) await wasmInit;

    switch (cmd) {

        // ── Core document transforms ──────────────────────────────────────
        case 'md_to_html':
            return md_to_html(args.markdown ?? '');

        case 'html_to_md':
            return html_to_md(args.html ?? '');

        // ── File I/O: replaced with browser-native File API / localStorage ─
        case 'import_file': {
            // Open a native browser file picker.
            const md = await _pickerToText([
                '.md', '.html', '.htm', '.docx', '.odt', '.pdf',
                '.typ', '.rtf', '.pptx', '.epub', '.tex'
            ]);
            const filename = md._name ?? 'document.md';
            // For non-md files we cannot run the Rust parsers in browser;
            // return raw text and let the editor handle it gracefully.
            return [md._content, filename, ''];
        }

        case 'save_file': {
            // Download the markdown as a .md file via a data-URL anchor.
            const content = args.markdown ?? '';
            const name    = (args.current_path?.split(/[\\/]/).pop()) || 'document.md';
            _downloadText(content, name, 'text/markdown');
            return '';  // path — not meaningful in browser
        }

        case 'save_as_format': {
            const md  = args.markdown ?? '';
            const ext = _formatToExt(args.format ?? 'markdown');
            if (['docx', 'pptx', 'epub', 'rtf', 'typst', 'html'].includes(ext)) {
                try {
                    const bytes = export_document(md, ext);
                    _downloadBlob(bytes, `${args.doc_name ?? 'document'}.${ext}`, 'application/octet-stream');
                    return;
                } catch (e) {
                    console.error("WASM export failed", e);
                }
            }
            // Fallback for PDF or unsupported
            _downloadText(md, `${args.doc_name ?? 'document'}.md`, 'text/plain');
            return;
        }

        case 'export_format': {
            const md   = args.markdown ?? '';
            const fmt  = args.format ?? 'markdown';
            const ext  = _formatToExt(fmt);
            const name = `${args.doc_name ?? 'document'}.${ext}`;
            if (['docx', 'pptx', 'epub', 'rtf', 'typst', 'html'].includes(ext)) {
                try {
                    const bytes = export_document(md, ext);
                    const b64 = _bytesToBase64(bytes);
                    return [b64, 'application/octet-stream', name];
                } catch (e) {
                    console.error("WASM export failed", e);
                }
            }
            const b64  = btoa(unescape(encodeURIComponent(md)));
            return [b64, 'text/plain', `${args.doc_name ?? 'document'}.md`];
        }

        // ── Autosave via localStorage ─────────────────────────────────────
        case 'autosave_file': {
            const key = `${LOCAL_AUTOSAVE_KEY}:${args.doc_name ?? 'default'}`;
            localStorage.setItem(key, args.markdown ?? '');
            localStorage.setItem(`${key}:time`, Date.now().toString());
            return;
        }

        case 'load_latest_autosave': {
            // Find the most-recently-written autosave key in localStorage.
            let latestKey = null;
            let latestTime = 0;
            for (let i = 0; i < localStorage.length; i++) {
                const k = localStorage.key(i);
                if (!k?.startsWith(LOCAL_AUTOSAVE_KEY + ':') || k.endsWith(':time')) continue;
                const t = parseInt(localStorage.getItem(k + ':time') ?? '0', 10);
                if (t > latestTime) { latestTime = t; latestKey = k; }
            }
            if (!latestKey) return null;
            const content = localStorage.getItem(latestKey) ?? '';
            const name    = latestKey.replace(`${LOCAL_AUTOSAVE_KEY}:`, '');
            return [content, name];
        }

        // ── Printing ──────────────────────────────────────────────────────
        case 'print_pdf':
            // Fall back to browser print dialog.
            window.print();
            return;

        // ── Open existing file by absolute path ───────────────────────────
        case 'open_file_by_path': {
            // Absolute paths are not accessible in browser; treat as no-op.
            console.warn('[WASM bridge] open_file_by_path: not available in browser mode.');
            return ['', '', args.path ?? ''];
        }

        // ── System fonts ──────────────────────────────────────────────────
        case 'get_system_fonts':
            // queryLocalFonts is available in Chromium 103+ with permission.
            if (window.queryLocalFonts) {
                try {
                    const fonts = await window.queryLocalFonts();
                    return [...new Set(fonts.map(f => f.family))].sort();
                } catch { /* permission denied or unsupported */ }
            }
            return [
                'Arial', 'Georgia', 'Times New Roman', 'Courier New',
                'Verdana', 'Trebuchet MS', 'Palatino', 'Garamond',
                'Bookman Old Style', 'Comic Sans MS', 'Impact', 'Tahoma',
            ];

        // ── Asset storage ─────────────────────────────────────────────────
        case 'save_base64_asset': {
            // Nothing meaningful to do in browser without a server;
            // return the data URL directly so <img> tags continue to work.
            const dataUrl = args.base64_data.startsWith('data:')
                ? args.base64_data
                : `data:application/${args.ext ?? 'bin'};base64,${args.base64_data}`;
            return dataUrl;
        }

        // ── Diff ──────────────────────────────────────────────────────────
        case 'generate_diff': {
            // Inline diff not available in WASM build (tree-sitter C bindings omitted).
            // Return a simple placeholder showing both versions.
            const oldHtml = md_to_html(args.old_md ?? '');
            const newHtml = md_to_html(args.new_md ?? '');
            return `<p><em>[Browser diff unavailable — WASM build omits tree-sitter C bindings]</em></p>
<h3>Original</h3>${oldHtml}<h3>Revised</h3>${newHtml}`;
        }

        // ── marksmen-cite integration ─────────────────────────────────────
        case 'load_marksmen_cite_db': {
            return localStorage.getItem(LOCAL_SOURCES_KEY) ?? '[]';
        }

        case 'format_csl_citation': {
            try {
                // APA is default if no style XML provided
                const style_xml = args.style_xml || '';
                return format_csl_citation(JSON.stringify([args.citation ?? {}]), style_xml);
            } catch (e) {
                console.error("WASM CSL failed", e);
                const c = args.citation ?? {};
                return `(${c.author?.split(',')[0].trim() ?? 'Unknown'}, ${c.year ?? 'n.d.'})`;
            }
        }

        case 'format_csl_bibliography': {
            try {
                const style_xml = args.style_xml || '';
                return format_csl_bibliography(JSON.stringify(args.citations ?? []), style_xml);
            } catch (e) {
                console.error("WASM CSL failed", e);
                return (args.citations ?? []).map(c => 
                    `<p>${c.author ?? 'Unknown'} (${c.year ?? 'n.d.'}). <em>${c.title ?? 'Untitled'}</em>. ${c.publisher ?? ''}</p>`
                ).join('\n');
            }
        }

        // ── Mail merge ───────────────────────────────────────────────────
        case 'execute_mail_merge': {
            const md = args.template_markdown ?? '';
            const csv_data = args.csv_data ?? '';
            const fmt = args.format ?? 'docx';
            const ext = _formatToExt(fmt);
            const docName = args.doc_name ?? 'merged';
            
            if (['docx', 'pptx', 'epub', 'rtf', 'typst', 'html'].includes(ext)) {
                try {
                    const bytes = wasm_execute_mail_merge(md, csv_data, ext);
                    _downloadBlob(bytes, `${docName}.${ext}`, 'application/octet-stream');
                    return;
                } catch (e) {
                    console.error("WASM mail merge failed", e);
                    throw e; // Bubble error up to UI
                }
            }
            
            // Fallback
            _downloadText(md, `${docName}.md`, 'text/markdown');
            console.warn(`[WASM bridge] execute_mail_merge: unsupported format ${fmt}; downloaded raw Markdown instead.`);
            return;
        }

        // ── Export binder ─────────────────────────────────────────────────
        case 'export_binder': {
            console.warn('[WASM bridge] export_binder: unavailable in browser mode.');
            return;
        }

        default:
            console.warn(`[WASM bridge] Unhandled command '${cmd}' in browser mode.`);
            return null;
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/**
 * Opens a file picker restricted to `extensions` and resolves with
 * { _content: string, _name: string }.
 */
function _pickerToText(extensions) {
    return new Promise((resolve, reject) => {
        const input = document.createElement('input');
        input.type   = 'file';
        input.accept = extensions.join(',');
        input.onchange = e => {
            const file = e.target.files[0];
            if (!file) { reject(new Error('No file selected')); return; }
            const fr = new FileReader();
            fr.onload = ev => resolve({ _content: ev.target.result, _name: file.name });
            fr.onerror = () => reject(new Error('FileReader error'));
            fr.readAsText(file);
        };
        input.oncancel = () => reject(new Error('No file selected'));
        input.click();
    });
}

/** Trigger a browser download for text content. */
function _downloadText(text, filename, mimeType) {
    const blob = new Blob([text], { type: mimeType });
    const url  = URL.createObjectURL(blob);
    const a    = document.createElement('a');
    a.href     = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
}

/** Map export format string to file extension. */
function _formatToExt(format) {
    const map = {
        markdown: 'md', docx: 'docx', pdf: 'pdf',
        html: 'html', typst: 'typ', pptx: 'pptx',
        epub: 'epub', rtf: 'rtf', odt: 'odt',
    };
    return map[format] ?? format;
}

/** Trigger a browser download for binary content. */
function _downloadBlob(bytes, filename, mimeType) {
    const blob = new Blob([bytes], { type: mimeType });
    const url  = URL.createObjectURL(blob);
    const a    = document.createElement('a');
    a.href     = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
}

/** Convert a Uint8Array to a Base64 string. */
function _bytesToBase64(bytes) {
    let binary = '';
    const len = bytes.byteLength;
    for (let i = 0; i < len; i++) {
        binary += String.fromCharCode(bytes[i]);
    }
    return window.btoa(binary);
}
