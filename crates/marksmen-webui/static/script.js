const input = document.getElementById('markdown-input');
const statusIndicator = document.getElementById('status-indicator');
const tabs = document.querySelectorAll('.tab');

let timeout = null;

// ── Tab switching ────────────────────────────────────────────────────────────
tabs.forEach(tab => {
    tab.addEventListener('click', () => {
        tabs.forEach(t => t.classList.remove('active'));
        document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));

        tab.classList.add('active');
        const target = tab.getAttribute('data-target');
        document.getElementById(`${target}-content`).classList.add('active');
    });
});

// ── Debounced compile on input ───────────────────────────────────────────────
input.addEventListener('input', () => {
    statusIndicator.textContent = 'Editing...';
    statusIndicator.classList.remove('syncing');
    clearTimeout(timeout);
    timeout = setTimeout(compileMarksmen, 800);
});

// ── Core compile function ────────────────────────────────────────────────────
async function compileMarksmen() {
    const md = input.value;
    if (!md.trim()) return;

    statusIndicator.textContent = 'Compiling...';
    statusIndicator.classList.add('syncing');

    try {
        const response = await fetch('/api/inspect', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ markdown: md })
        });

        if (!response.ok) throw new Error(`HTTP ${response.status}`);

        const data = await response.json();

        // ── Populate preview iframes via srcdoc ──────────────────────────
        setIframe('html-content', data.preview_html);
        setIframe('typst-content', data.preview_typst_svg);
        setIframe('docx-content', data.preview_docx);
        setIframe('odt-content', data.preview_odt);

        // ── PDF: embed as base64 data URI ────────────────────────────────
        const pdfEmbed = document.getElementById('pdf-embed');
        if (data.preview_pdf_b64 && !data.preview_pdf_b64.startsWith('PDF Error')) {
            pdfEmbed.src = `data:application/pdf;base64,${data.preview_pdf_b64}`;
        } else {
            pdfEmbed.removeAttribute('src');
            document.getElementById('pdf-content').textContent =
                data.preview_pdf_b64 || 'PDF unavailable';
        }

        // ── Populate source panes ────────────────────────────────────────
        document.getElementById('ast-content').textContent      = data.ast;
        document.getElementById('html-src-content').textContent = data.html_src;
        document.getElementById('typst-src-content').textContent = data.typst_src;
        document.getElementById('docx-xml-content').textContent = data.docx_xml;
        document.getElementById('odt-xml-content').textContent  = data.odt_xml;

        statusIndicator.textContent = 'Synced';
        statusIndicator.classList.remove('syncing');
    } catch (e) {
        statusIndicator.textContent = 'Error';
        statusIndicator.classList.remove('syncing');
        console.error('Marksmen compile failed:', e);
    }
}

function setIframe(id, html) {
    const iframe = document.getElementById(id);
    // srcdoc is XSS-safe inside sandbox="allow-same-origin"
    iframe.srcdoc = html;
}

// Initial compile on page load
compileMarksmen();
