/**
 * Voice Dictation Module for Marksmen Editor
 * Utilizes the browser's native Web Speech API.
 */

export class DictationEngine {
    constructor(editorElement) {
        this.editor = editorElement;
        this.isListening = false;
        this.recognition = null;
        this.toggleBtn = null;
        
        this.initSpeechRecognition();
    }

    initSpeechRecognition() {
        const SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition;
        if (!SpeechRecognition) {
            console.warn("Web Speech API is not supported in this browser.");
            return;
        }

        this.recognition = new SpeechRecognition();
        this.recognition.continuous = true;
        this.recognition.interimResults = true;
        this.recognition.lang = 'en-US';

        this.recognition.onstart = () => {
            this.isListening = true;
            this.updateButtonUI(true);
        };

        this.recognition.onresult = (event) => {
            let interimTranscript = '';
            let finalTranscript = '';

            for (let i = event.resultIndex; i < event.results.length; ++i) {
                if (event.results[i].isFinal) {
                    finalTranscript += event.results[i][0].transcript;
                } else {
                    interimTranscript += event.results[i][0].transcript;
                }
            }

            if (finalTranscript) {
                this.insertTextAtCursor(finalTranscript + ' ');
            }
            
            // Optionally, we could show interimTranscript in a floating tooltip
        };

        this.recognition.onerror = (event) => {
            console.error("Speech recognition error", event.error);
            this.stopListening();
        };

        this.recognition.onend = () => {
            // Restart if we are supposed to be listening (continuous fallback)
            if (this.isListening) {
                try {
                    this.recognition.start();
                } catch(e) {
                    this.isListening = false;
                    this.updateButtonUI(false);
                }
            } else {
                this.updateButtonUI(false);
            }
        };
    }

    insertTextAtCursor(text) {
        if (!this.editor) return;
        this.editor.focus();
        
        const selection = window.getSelection();
        if (!selection.rangeCount) return;
        
        const range = selection.getRangeAt(0);
        range.deleteContents();
        
        const textNode = document.createTextNode(text);
        range.insertNode(textNode);
        
        // Move cursor to the end of inserted text
        range.setStartAfter(textNode);
        range.collapse(true);
        selection.removeAllRanges();
        selection.addRange(range);
        
        // Dispatch input event to trigger auto-save or reactivity
        this.editor.dispatchEvent(new Event('input', { bubbles: true }));
    }

    toggle() {
        if (!this.recognition) {
            alert("Voice Dictation is not supported by your browser.");
            return;
        }

        if (this.isListening) {
            this.stopListening();
        } else {
            this.startListening();
        }
    }

    startListening() {
        if (this.recognition && !this.isListening) {
            try {
                this.recognition.start();
            } catch (e) {
                console.error("Failed to start speech recognition:", e);
            }
        }
    }

    stopListening() {
        if (this.recognition && this.isListening) {
            this.isListening = false;
            this.recognition.stop();
            this.updateButtonUI(false);
        }
    }

    attachButton(buttonId) {
        const btn = document.getElementById(buttonId);
        if (btn) {
            this.toggleBtn = btn;
            btn.addEventListener('click', (e) => {
                e.preventDefault();
                this.toggle();
            });
        }
    }

    updateButtonUI(active) {
        if (!this.toggleBtn) return;
        
        if (active) {
            this.toggleBtn.classList.add('active');
            this.toggleBtn.style.color = 'red';
            this.toggleBtn.title = "Stop Dictation";
        } else {
            this.toggleBtn.classList.remove('active');
            this.toggleBtn.style.color = '';
            this.toggleBtn.title = "Start Dictation";
        }
    }
}
