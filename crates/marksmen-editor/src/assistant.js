/**
 * AI Assistant / Grammar Suggestions Module
 */

import { invoke } from './wasm_bridge.js';

export class AssistantEngine {
    constructor(editorElement) {
        this.editor = editorElement;
        this.isProcessing = false;
        this.suggestionPanel = this.createPanel();
    }

    createPanel() {
        const panel = document.createElement('div');
        panel.id = 'ai-assistant-panel';
        panel.hidden = true;
        Object.assign(panel.style, {
            position: 'absolute',
            top: '80px',
            right: '20px',
            width: '300px',
            maxHeight: '400px',
            background: 'var(--bg-secondary)',
            border: '1px solid var(--border)',
            borderRadius: '6px',
            boxShadow: '0 4px 12px var(--shadow)',
            zIndex: '1000',
            overflowY: 'auto',
            padding: '10px',
            fontFamily: 'var(--font-family)',
            fontSize: '12px',
            color: 'var(--text-primary)'
        });
        document.body.appendChild(panel);
        return panel;
    }

    async analyzeDocument() {
        if (this.isProcessing || !this.editor) return;
        this.isProcessing = true;
        this.suggestionPanel.hidden = false;
        this.suggestionPanel.innerHTML = '<p>Analyzing text with Local AI Engine...</p>';

        try {
            const textContent = this.editor.innerText || this.editor.textContent;
            
            // Invoke the IPC bridge hook to run the grammar check
            const suggestions = await invoke('request_grammar_check', { text: textContent });
            
            this.renderSuggestions(suggestions);
        } catch (err) {
            console.error("AI Assistant analysis failed:", err);
            this.suggestionPanel.innerHTML = '<p style="color:red;">Error analyzing document.</p>';
        } finally {
            this.isProcessing = false;
        }
    }

    renderSuggestions(suggestions) {
        if (!suggestions || suggestions.length === 0) {
            this.suggestionPanel.innerHTML = `
                <div style="display:flex; justify-content:space-between; align-items:center;">
                    <b>Grammar & Clarity</b>
                    <button id="close-assistant" style="background:none;border:none;cursor:pointer;">✖</button>
                </div>
                <p style="color:var(--text-hint); margin-top:10px;">Looking good! No suggestions found.</p>
            `;
            document.getElementById('close-assistant').addEventListener('click', () => {
                this.suggestionPanel.hidden = true;
            });
            return;
        }

        let html = `
            <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:10px;">
                <b>Grammar & Clarity (${suggestions.length})</b>
                <button id="close-assistant" style="background:none;border:none;cursor:pointer;">✖</button>
            </div>
            <div style="display:flex; flex-direction:column; gap:8px;">
        `;

        suggestions.forEach((sug, i) => {
            html += `
                <div style="background:var(--bg-primary); border:1px solid var(--border); border-left:3px solid #f08c00; padding:8px; border-radius:4px;">
                    <div style="color:var(--text-secondary); margin-bottom:4px;">Suggestion: "${sug.matched}" → <b>${sug.correction}</b></div>
                    <div style="color:var(--text-hint); font-style:italic;">${sug.message}</div>
                </div>
            `;
        });

        html += `</div>`;
        this.suggestionPanel.innerHTML = html;

        document.getElementById('close-assistant').addEventListener('click', () => {
            this.suggestionPanel.hidden = true;
        });
    }

    attachButton(buttonId) {
        const btn = document.getElementById(buttonId);
        if (btn) {
            btn.addEventListener('click', (e) => {
                e.preventDefault();
                if (this.suggestionPanel.hidden) {
                    this.analyzeDocument();
                } else {
                    this.suggestionPanel.hidden = true;
                }
            });
        }
    }
}
