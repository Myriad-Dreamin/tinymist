import {
    createConnection,
    BrowserMessageReader,
    BrowserMessageWriter,
    TextDocuments,
    TextDocumentSyncKind,
    InitializeResult,
    DocumentSymbolParams,
    ReferenceParams,
    DefinitionParams,
    FoldingRangeParams,
    SelectionRangeParams,
    DocumentHighlightParams,
    SemanticTokensParams,
    SemanticTokensDeltaParams,
    DocumentFormattingParams,
    InlayHintParams,
    DocumentColorParams,
    DocumentLinkParams,
    ColorPresentationParams,
    CodeActionParams,
    CodeLensParams,
    CompletionParams,
    SignatureHelpParams,
    RenameParams,
    Position,
    TextDocumentPositionParams
} from 'vscode-languageserver/browser';

import { TextDocument } from 'vscode-languageserver-textdocument';
// Import the WASM module
import { TinymistLanguageServer } from "./pkg/tinymist_wasm";

// Initialize WASM and create language server instance
let server: TinymistLanguageServer;

// Create a connection for the server
const reader = new BrowserMessageReader(self as any);
const writer = new BrowserMessageWriter(self as any);
const connection = createConnection(reader, writer);

// Create a document manager
const documents = new TextDocuments(TextDocument);

console.log('Language server worker starting...');

// Initialize the server
server = new TinymistLanguageServer();
console.log(`Tinymist WASM language server v${server.version()} initialized`);
console.log(server.greet());

connection.onInitialize((_params) => {
    // Define server capabilities
    const capabilities = {
        textDocumentSync: TextDocumentSyncKind.Incremental,
        completionProvider: {
            resolveProvider: false,
            triggerCharacters: ['.', '#', '@', '=', ':', ',', '(', '[', '{']
        },
        hoverProvider: true,
        documentSymbolProvider: true,
        definitionProvider: true,
        declarationProvider: true,
        referencesProvider: true,
        documentHighlightProvider: true,
        documentFormattingProvider: true,
        documentRangeFormattingProvider: true,
        foldingRangeProvider: true,
        selectionRangeProvider: true,
        semanticTokensProvider: {
            full: true,
            range: false,
            legend: {
                tokenTypes: [
                    'comment', 'string', 'keyword', 'number', 'regexp', 'operator', 
                    'namespace', 'type', 'struct', 'class', 'interface', 'enum',
                    'typeParameter', 'function', 'method', 'decorator', 'macro',
                    'variable', 'parameter', 'property', 'label'
                ],
                tokenModifiers: [
                    'declaration', 'definition', 'readonly', 'static', 'deprecated',
                    'abstract', 'async', 'modification', 'documentation', 'defaultLibrary'
                ]
            }
        },
        colorProvider: true,
        documentLinkProvider: {
            resolveProvider: false
        },
        codeActionProvider: {
            codeActionKinds: ['quickfix', 'refactor', 'source']
        },
        codeLensProvider: {
            resolveProvider: false
        },
        renameProvider: {
            prepareProvider: true
        },
        inlayHintProvider: true,
        workspaceSymbolProvider: true
    };
    
    return {
        capabilities,
        serverInfo: {
            name: "Tinymist Language Server",
            version: server.version()
        }
    } as InitializeResult;
});

// Document management
documents.onDidOpen(event => {
    const document = event.document;
    server.update_document(document.uri, document.getText());
});

documents.onDidChangeContent(change => {
    const document = change.document;
    server.update_document(document.uri, document.getText());
});

documents.onDidClose(event => {
    server.remove_document(event.document.uri);
});

// Handle completion requests
connection.onCompletion((textDocumentPosition) => {
    try {
        const completions = server.get_completions(
            textDocumentPosition.textDocument.uri,
            textDocumentPosition.position.line,
            textDocumentPosition.position.character
        );
        return completions as any[];
    } catch (e) {
        console.error('Completion error:', e);
        return [];
    }
});

// Handle hover requests
connection.onHover((params) => {
    try {
        return server.get_hover(
            params.textDocument.uri,
            params.position.line,
            params.position.character
        );
    } catch (e) {
        console.error('Hover error:', e);
        return null;
    }
});

// Handle document symbols
connection.onDocumentSymbol((params: DocumentSymbolParams) => {
    try {
        return server.get_document_symbols(params.textDocument.uri) as any[];
    } catch (e) {
        console.error('Document symbols error:', e);
        return [];
    }
});

// Go to definition
connection.onDefinition((params: DefinitionParams) => {
    try {
        return server.goto_definition(
            params.textDocument.uri,
            params.position.line,
            params.position.character
        );
    } catch (e) {
        console.error('Definition error:', e);
        return null;
    }
});

// Go to declaration
connection.onDeclaration((params) => {
    try {
        return server.goto_declaration(
            params.textDocument.uri,
            params.position.line,
            params.position.character
        );
    } catch (e) {
        console.error('Declaration error:', e);
        return null;
    }
});

// Find references
connection.onReferences((params: ReferenceParams) => {
    try {
        return server.find_references(
            params.textDocument.uri,
            params.position.line,
            params.position.character
        );
    } catch (e) {
        console.error('References error:', e);
        return [];
    }
});

// Folding ranges
connection.onFoldingRanges((params: FoldingRangeParams) => {
    try {
        return server.folding_range(params.textDocument.uri);
    } catch (e) {
        console.error('Folding range error:', e);
        return [];
    }
});

// Selection ranges
connection.onSelectionRanges((params) => {
    try {
        // Convert positions to a format the WASM module can understand
        return server.selection_range(
            params.textDocument.uri,
            params.positions
        );
    } catch (e) {
        console.error('Selection range error:', e);
        return [];
    }
});

// Document highlights
connection.onDocumentHighlight((params: DocumentHighlightParams) => {
    try {
        return server.document_highlight(
            params.textDocument.uri,
            params.position.line,
            params.position.character
        );
    } catch (e) {
        console.error('Document highlight error:', e);
        return [];
    }
});

// Semantic tokens
connection.onRequest('textDocument/semanticTokens/full', (params: SemanticTokensParams) => {
    try {
        return server.semantic_tokens_full(params.textDocument.uri);
    } catch (e) {
        console.error('Semantic tokens error:', e);
        return null;
    }
});

// Semantic tokens delta
connection.onRequest('textDocument/semanticTokens/range', () => null); // Not implemented yet
connection.onRequest('textDocument/semanticTokens/full/delta', (params: SemanticTokensDeltaParams) => {
    try {
        return server.semantic_tokens_delta(
            params.textDocument.uri,
            params.previousResultId
        );
    } catch (e) {
        console.error('Semantic tokens delta error:', e);
        return null;
    }
});

// Document formatting
connection.onDocumentFormatting((params: DocumentFormattingParams) => {
    try {
        return server.formatting(params.textDocument.uri);
    } catch (e) {
        console.error('Document formatting error:', e);
        return [];
    }
});

// Inlay hints
connection.onRequest('textDocument/inlayHint', (params: InlayHintParams) => {
    try {
        return server.inlay_hint(
            params.textDocument.uri,
            params.range.start.line,
            params.range.start.character,
            params.range.end.line,
            params.range.end.character
        );
    } catch (e) {
        console.error('Inlay hint error:', e);
        return [];
    }
});

// Document colors
connection.onDocumentColor((params: DocumentColorParams) => {
    try {
        return server.document_color(params.textDocument.uri);
    } catch (e) {
        console.error('Document color error:', e);
        return [];
    }
});

// Document links
connection.onDocumentLinks((params: DocumentLinkParams) => {
    try {
        return server.document_link(params.textDocument.uri);
    } catch (e) {
        console.error('Document link error:', e);
        return [];
    }
});

// Color presentations
connection.onColorPresentation((params: ColorPresentationParams) => {
    try {
        return server.color_presentation(
            params.textDocument.uri,
            params.color,
            params.range.start.line,
            params.range.start.character,
            params.range.end.line,
            params.range.end.character
        );
    } catch (e) {
        console.error('Color presentation error:', e);
        return [];
    }
});

// Code actions
connection.onCodeAction((params: CodeActionParams) => {
    try {
        return server.code_action(
            params.textDocument.uri,
            params.range.start.line,
            params.range.start.character,
            params.range.end.line,
            params.range.end.character,
            params.context
        );
    } catch (e) {
        console.error('Code action error:', e);
        return [];
    }
});

// Code lenses
connection.onCodeLens((params) => {
    try {
        return server.code_lens(params.textDocument.uri);
    } catch (e) {
        console.error('Code lens error:', e);
        return [];
    }
});

// Signature help
connection.onSignatureHelp((params) => {
    try {
        return server.signature_help(
            params.textDocument.uri,
            params.position.line,
            params.position.character
        );
    } catch (e) {
        console.error('Signature help error:', e);
        return null;
    }
});

// Rename
connection.onRenameRequest((params: RenameParams) => {
    try {
        return server.rename(
            params.textDocument.uri,
            params.position.line,
            params.position.character,
            params.newName
        );
    } catch (e) {
        console.error('Rename error:', e);
        return null;
    }
});

// Prepare rename
connection.onPrepareRename((params: TextDocumentPositionParams) => {
    try {
        return server.prepare_rename(
            params.textDocument.uri,
            params.position.line,
            params.position.character
        );
    } catch (e) {
        console.error('Prepare rename error:', e);
        return null;
    }
});

// Workspace symbols
connection.onWorkspaceSymbol((params) => {
    try {
        return server.symbol(params.query);
    } catch (e) {
        console.error('Workspace symbol error:', e);
        return [];
    }
});

// Custom "onEnter" handler
connection.onRequest('experimental/onEnter', (params: any) => {
    try {
        return server.on_enter(
            params.textDocument.uri,
            params.range.start.line,
            params.range.start.character,
            params.range.end.line,
            params.range.end.character
        );
    } catch (e) {
        console.error('OnEnter error:', e);
        return null;
    }
});

// Will rename files
connection.onRequest('workspace/willRenameFiles', (params: any) => {
    try {
        return server.will_rename_files(params.files);
    } catch (e) {
        console.error('Will rename files error:', e);
        return null;
    }
});

// Handle document diagnostics
documents.onDidChangeContent((change) => {
    // In the future, we would get diagnostics from the WASM module
    connection.sendDiagnostics({
        uri: change.document.uri,
        diagnostics: []
    });
});

// Start the language server
documents.listen(connection);
connection.listen();

console.log('Language server worker running...');
