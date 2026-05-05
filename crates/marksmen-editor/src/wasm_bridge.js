import init, { md_to_html, html_to_md } from './wasm/marksmen_wasm.js';

let wasmReady = false;

// Initialize WASM
init().then(() => {
    wasmReady = true;
    console.log("Marksmen WASM bridge initialized.");
});

export async function invoke(cmd, args) {
    if (window.__TAURI__) {
        return window.__TAURI__.core.invoke(cmd, args);
    }
    
    if (!wasmReady) {
        throw new Error("WASM module not yet initialized.");
    }
    
    // Polyfills for browser mode
    switch (cmd) {
        case 'md_to_html':
            return md_to_html(args.markdown);
        case 'html_to_md':
            return html_to_md(args.html);
        case 'read_file':
            // Reading files from disk isn't supported in browser.
            // But we can fallback to localStorage or just return empty
            return localStorage.getItem('mock_file_' + args.path) || "";
        case 'write_file':
            localStorage.setItem('mock_file_' + args.path, args.content);
            return;
        case 'get_recent_files':
            return [];
        default:
            console.warn(`WASM fallback for command '${cmd}' is not implemented.`);
            return null;
    }
}
