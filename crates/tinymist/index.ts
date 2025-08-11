import 'monaco-editor/esm/vs/editor/editor.api.js';
import { editor, languages } from 'monaco-editor/esm/vs/editor/editor.api.js';
import { MonacoLanguageClient } from 'monaco-languageclient';
import {CloseAction, ErrorAction} from 'vscode-languageclient';
import { BrowserMessageReader, BrowserMessageWriter } from 'vscode-languageserver-protocol/browser';

const LANGUAGE_ID = 'typst';

languages.register({
    id: LANGUAGE_ID,
    extensions: ['.typ'],
    aliases: ['typst', 'tynimist'],
    mimetypes: ['text/vnd.typst'],
});

export function createMonacoLanguageClient(worker: Worker) {
    const reader = new BrowserMessageReader(worker);
    const writer = new BrowserMessageWriter(worker);
    // Create a MonacoLanguageClient instance
    const languageClient = new MonacoLanguageClient({
        name: 'Tynimist Language Client',
        clientOptions: {
            // The language ID to activate this client for
            documentSelector: [LANGUAGE_ID],
            // Don't show an error message if the server crashes
            errorHandler: {
                error: () => ({ action: ErrorAction.Continue }), // Continue
                closed: () => ({ action: CloseAction.DoNotRestart }) // Do not restart
            }
        },
        // The transport protocol to use for communication with the worker
        messageTransports: {
            reader,
            writer
        }
    });

    return languageClient;
}
