# Goals

I want to port the tinymist functionalities to WASM, so that I can run it purely within the browser.

Eventually, I want to port functionalities not only the LSP, but also preview compiler to HTML output.
However, for now, I want to focus on the LSP functionalities, so that I can use it in the browser paired with Monaco Editor. Keep in mind that you structure the code in a way that it can be extended later on to support other functionalities as well.

In doing so, I want to create a Typescript package with the following tech stack:
- `tinymist-wasm` rust crate in the workspace 
  - wasm-pack to build the crate (not for `--target web` but for `--target bundler`, or no target option because it defaults to bundler)
    - i.e. `wasm-pack build --target bundler crates/tinymist-wasm`
    - the compiled artifacts (wasm and glue js) will be placed in `crates/tinymist-wasm/pkg`
- Within the same folder as `tinymist-wasm` crate, make `tinymist-monaco` npm package that imports the wasm and glue js from the `tinymist-wasm` output.

The implementation of `tinymist-monaco` package should somewhat look like this:

`tinymist-monaco/index.ts`:
```ts
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

const reader = new BrowserMessageReader(worker);
const writer = new BrowserMessageWriter(worker);

export function createMonacoLanguageClient(worker: Worker) {
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
```


`tinymist-monaco/worker.ts`:
```ts
import {
    createConnection,
    BrowserMessageReader,
    BrowserMessageWriter,
    TextDocuments,
    Diagnostic,
    DiagnosticSeverity,
    CompletionItem,
    CompletionItemKind,
    TextDocumentSyncKind,
    InitializeResult
} from 'vscode-languageserver/browser';

import { TextDocument } from 'vscode-languageserver-textdocument';
import init, { TinymistLanguageServer } from "tinymist-wasm";

await init();
const server = new TinymistLanguageServer(/* options */); // use it in the event handlers below

console.log('Language server worker running...');

// --- 1. Create a connection for the server ---
// The connection uses the browser's postMessage/onmessage functions to communicate
// with the main thread.
const reader = new BrowserMessageReader(self);
const writer = new BrowserMessageWriter(self);
const connection = createConnection(reader, writer);

// --- 2. Create a simple text document manager ---
const documents = new TextDocuments(TextDocument);

// --- 3. Define server capabilities ---
connection.onInitialize((params) => {
   // ...
});

// --- 4. Set up event handlers for LSP features ---

// This handler provides validation of the text document.
documents.onDidChangeContent(change => {
    // ...
});

// register all the `onCompletion`, `onHover`, ... and so on, that can be found in `crates/tinymist/src/lsp/query.rs`, the implementation of the `ServerState`
connection.onCompletion((_textDocumentPosition) => {
    // ....
});


// --- 5. Start the server ---
documents.listen(connection);
connection.listen();
```

Where the user can use it like this:

```ts
import { createMonacoLanguageClient } from 'tinymist-monaco';
import TinymistWorker from 'tinymist-monaco/worker?worker';

// assuming you already have editor instance
let editorInstance: monaco.editor.IStandaloneCodeEditor;

const worker = new TinymistWorker();
const languageClient = createMonacoLanguageClient(worker);
languageClient.start();
```


## Notes

**Important**

* **Ignore** the `web.rs` file in the `tinymist-core` crate, the wasm interface code should not be in the core crate.
* We are doing things differently here, and you may delete the `web.rs` file later.


