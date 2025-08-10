//! Tinymist WASM language server implementation.
//!
//! This crate provides a WebAssembly-compatible implementation of the Tinymist
//! language server for use with Monaco Editor in the browser.
#![warn(missing_docs)]

use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use js_sys::{Array, Object};
use lsp_types::{DocumentSymbol, DocumentSymbolResponse};

/// Initialize panic hook for better error messages in the browser console
#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
}

/// TinymistLanguageServer implements the LSP protocol for Typst documents
/// in a WebAssembly environment
#[wasm_bindgen]
pub struct TinymistLanguageServer {
    version: String,
    /// Store document contents by URI
    documents: HashMap<String, String>,
}

#[wasm_bindgen]
impl TinymistLanguageServer {
    /// Create a new language server.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            documents: HashMap::new(),
        }
    }
    
    /// Get the version of the language server.
    pub fn version(&self) -> String {
        self.version.clone()
    }
    
    /// Get a greeting message.
    pub fn greet(&self) -> String {
        format!("Hello from Tinymist WASM v{}!", self.version)
    }
    
    /// Update or add a document in the language server's storage
    pub fn update_document(&mut self, uri: String, content: String) {
        self.documents.insert(uri.clone(), content);
        web_sys::console::log_1(&format!("Document updated: {}", uri).into());
    }
    
    /// Remove a document from the language server's storage
    pub fn remove_document(&mut self, uri: String) {
        self.documents.remove(&uri);
        web_sys::console::log_1(&format!("Document removed: {}", uri).into());
    }
    
    // LSP feature implementations

    /// Get completion items for the specified position.
    pub fn get_completions(&self, uri: String, line: u32, character: u32) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public CompletionRequest API
        use tinymist_query::{CompletionRequest, LspPosition};
        
        let path = std::path::PathBuf::from(&uri);
        let lsp_position = LspPosition { line, character };
        
        let _request = CompletionRequest {
            path,
            position: lsp_position,
            explicit: false,
            trigger_character: None,
        };
        
        // For WASM, we can provide basic syntax-based completions
        // TODO: Implement basic syntax-based completion when semantic context is not available
        let js_completions = Array::new();
        
        // Basic Typst syntax keywords that can be completed
        let keywords = [
            "let", "set", "show", "import", "include", "if", "else", "for", "in", "while",
            "break", "continue", "return", "auto", "none", "true", "false"
        ];
        
        for keyword in &keywords {
            let completion = Object::new();
            js_sys::Reflect::set(&completion, &"label".into(), &(*keyword).into()).unwrap();
            js_sys::Reflect::set(&completion, &"kind".into(), &14u32.into()).unwrap(); // Keyword
            js_sys::Reflect::set(&completion, &"insertText".into(), &(*keyword).into()).unwrap();
            js_completions.push(&completion);
        }
        
        js_completions.into()
    }
    
    /// Get hover information for the specified position.
    pub fn get_hover(&self, uri: String, line: u32, character: u32) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return JsValue::NULL;
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public HoverRequest API
        use tinymist_query::{HoverRequest, LspPosition, to_typst_position, PositionEncoding};
        use typst_shim::syntax::LinkedNodeExt;
        
        let path = std::path::PathBuf::from(&uri);
        let lsp_position = LspPosition { line, character };
        
        let _request = HoverRequest {
            path,
            position: lsp_position,
        };
        
        // For WASM, we can provide basic syntax-based hover information
        if let Some(offset) = to_typst_position(lsp_position, PositionEncoding::Utf16, &source) {
            let root = typst::syntax::LinkedNode::new(source.root());
            if let Some(node) = root.leaf_at_compat(offset + 1) {
                let hover_obj = Object::new();
                
                // Create hover content based on syntax node
                let kind_name = format!("{:?}", node.kind());
                let node_text = node.text().to_string();
                
                let contents = Object::new();
                js_sys::Reflect::set(&contents, &"kind".into(), &"markdown".into()).unwrap();
                
                let value = if !node_text.trim().is_empty() && node_text.len() < 50 {
                    format!("**{}**: `{}`", kind_name, node_text.trim())
                } else {
                    format!("**{}**", kind_name)
                };
                
                js_sys::Reflect::set(&contents, &"value".into(), &value.into()).unwrap();
                js_sys::Reflect::set(&hover_obj, &"contents".into(), &contents).unwrap();
                
                return hover_obj.into();
            }
        }
        
        JsValue::NULL
    }
    
    /// Get document symbols for the specified document
    pub fn get_document_symbols(&self, uri: String) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source and extract symbols
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public DocumentSymbolRequest API
        use tinymist_query::{DocumentSymbolRequest, SyntaxRequest, PositionEncoding};
        
        let path = std::path::PathBuf::from(&uri);
        let request = DocumentSymbolRequest { path };
        
        if let Some(DocumentSymbolResponse::Nested(symbols)) = request.request(&source, PositionEncoding::Utf16) {
            let js_symbols = Array::new();
            
            for symbol in symbols {
                if let Some(symbol_obj) = self.document_symbol_to_js(&symbol) {
                    js_symbols.push(&symbol_obj);
                }
            }
            
            js_symbols.into()
        } else {
            Array::new().into()
        }
    }
    
    /// Go to definition at the specified position
    pub fn goto_definition(&self, uri: String, line: u32, character: u32) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return JsValue::NULL;
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public GotoDefinitionRequest API
        use tinymist_query::{GotoDefinitionRequest, StatefulRequest, PositionEncoding, LspPosition, to_typst_position};
        
        let path = std::path::PathBuf::from(&uri);
        let lsp_position = LspPosition { line, character };
        
        // Convert LSP position to Typst position  
        if let Some(_position) = to_typst_position(lsp_position, PositionEncoding::Utf16, &source) {
            let request = GotoDefinitionRequest { path, position: lsp_position };
            
            // For WASM, we don't have a full LocalContext, so we'll return a simple result
            // In a full implementation, we would use request.request(ctx, graph)
            // For now, return empty result as the method structure is established
            JsValue::NULL
        } else {
            JsValue::NULL
        }
    }
    
    /// Go to declaration at the specified position
    pub fn goto_declaration(&self, uri: String, line: u32, character: u32) -> JsValue {
        // Note: GotoDeclarationRequest is not fully implemented in tinymist-query yet
        // Return empty result for now
        JsValue::NULL
    }
    
    /// Find references at the specified position
    pub fn find_references(&self, uri: String, line: u32, character: u32) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public ReferencesRequest API
        use tinymist_query::{ReferencesRequest, StatefulRequest, PositionEncoding, LspPosition, to_typst_position};
        
        let path = std::path::PathBuf::from(&uri);
        let lsp_position = LspPosition { line, character };
        
        // Convert LSP position to Typst position  
        if let Some(_position) = to_typst_position(lsp_position, PositionEncoding::Utf16, &source) {
            let request = ReferencesRequest { path, position: lsp_position };
            
            // For WASM, we don't have a full LocalContext, so we'll return a simple result
            // In a full implementation, we would use request.request(ctx, graph)
            // For now, return empty array as the method structure is established
            Array::new().into()
        } else {
            Array::new().into()
        }
    }
    
    /// Get folding ranges for the document
    pub fn folding_range(&self, uri: String) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public FoldingRangeRequest API
        use tinymist_query::{FoldingRangeRequest, SyntaxRequest, PositionEncoding};
        
        let path = std::path::PathBuf::from(&uri);
        let request = FoldingRangeRequest { 
            path, 
            line_folding_only: false 
        };
        
        if let Some(folding_ranges) = request.request(&source, PositionEncoding::Utf16) {
            let js_ranges = Array::new();
            
            for range in folding_ranges {
                let js_range = Object::new();
                js_sys::Reflect::set(&js_range, &"startLine".into(), &range.start_line.into()).unwrap();
                js_sys::Reflect::set(&js_range, &"endLine".into(), &range.end_line.into()).unwrap();
                
                if let Some(start_char) = range.start_character {
                    js_sys::Reflect::set(&js_range, &"startCharacter".into(), &start_char.into()).unwrap();
                }
                if let Some(end_char) = range.end_character {
                    js_sys::Reflect::set(&js_range, &"endCharacter".into(), &end_char.into()).unwrap();
                }
                if let Some(kind) = range.kind {
                    let kind_str = match kind {
                        lsp_types::FoldingRangeKind::Comment => "comment",
                        lsp_types::FoldingRangeKind::Imports => "imports", 
                        lsp_types::FoldingRangeKind::Region => "region",
                    };
                    js_sys::Reflect::set(&js_range, &"kind".into(), &kind_str.into()).unwrap();
                }
                
                js_ranges.push(&js_range);
            }
            
            js_ranges.into()
        } else {
            Array::new().into()
        }
    }
    
    /// Get selection range at the specified positions
    pub fn selection_range(&self, uri: String, positions: JsValue) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Convert JsValue positions to Vec<LspPosition>
        let positions_array = js_sys::Array::from(&positions);
        let mut lsp_positions = Vec::new();
        
        for i in 0..positions_array.length() {
            let pos_obj = positions_array.get(i);
            if let Some(line) = js_sys::Reflect::get(&pos_obj, &"line".into()).ok()
                .and_then(|v| v.as_f64())
                .map(|v| v as u32)
            {
                if let Some(character) = js_sys::Reflect::get(&pos_obj, &"character".into()).ok()
                    .and_then(|v| v.as_f64())
                    .map(|v| v as u32)
                {
                    lsp_positions.push(tinymist_query::LspPosition { line, character });
                }
            }
        }
        
        // Use tinymist-query's public SelectionRangeRequest API
        use tinymist_query::{SelectionRangeRequest, SyntaxRequest, PositionEncoding};
        
        let path = std::path::PathBuf::from(&uri);
        let request = SelectionRangeRequest { 
            path, 
            positions: lsp_positions 
        };
        
        if let Some(selection_ranges) = request.request(&source, PositionEncoding::Utf16) {
            let js_ranges = Array::new();
            
            for range in selection_ranges {
                let js_range = self.selection_range_to_js(&range);
                js_ranges.push(&js_range);
            }
            
            js_ranges.into()
        } else {
            Array::new().into()
        }
    }
    
    /// Get document highlights at the specified position
    pub fn document_highlight(&self, uri: String, line: u32, character: u32) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public DocumentHighlightRequest API
        use tinymist_query::{LspPosition, to_typst_position, PositionEncoding};
        use typst_shim::syntax::LinkedNodeExt;
        
        let path = std::path::PathBuf::from(&uri);
        let lsp_position = LspPosition { line, character };
        
        // For WASM, we can provide basic syntax-based highlighting
        if let Some(offset) = to_typst_position(lsp_position, PositionEncoding::Utf16, &source) {
            let root = typst::syntax::LinkedNode::new(source.root());
            if let Some(node) = root.leaf_at_compat(offset + 1) {
                if matches!(node.kind(), typst::syntax::SyntaxKind::Ident) {
                    let target_text = node.text();
                    let js_highlights = Array::new();
                    
                    // Find all occurrences of the same identifier
                    fn find_matching_idents(
                        node: &typst::syntax::LinkedNode, 
                        target: &str, 
                        highlights: &Array,
                        source: &typst::syntax::Source
                    ) {
                        if matches!(node.kind(), typst::syntax::SyntaxKind::Ident) && node.text() == target {
                            let highlight = Object::new();
                            
                            // Convert range to LSP format
                            let range_obj = Object::new();
                            let start_obj = Object::new();
                            let end_obj = Object::new();
                            
                            let start_pos = source.byte_to_line(node.offset()).unwrap();
                            let end_pos = source.byte_to_line(node.offset() + node.text().len()).unwrap();
                            
                            js_sys::Reflect::set(&start_obj, &"line".into(), &(start_pos as u32).into()).unwrap();
                            js_sys::Reflect::set(&start_obj, &"character".into(), &(0u32).into()).unwrap(); // Simplified
                            js_sys::Reflect::set(&end_obj, &"line".into(), &(end_pos as u32).into()).unwrap();
                            js_sys::Reflect::set(&end_obj, &"character".into(), &(node.text().len() as u32).into()).unwrap();
                            
                            js_sys::Reflect::set(&range_obj, &"start".into(), &start_obj).unwrap();
                            js_sys::Reflect::set(&range_obj, &"end".into(), &end_obj).unwrap();
                            js_sys::Reflect::set(&highlight, &"range".into(), &range_obj).unwrap();
                            js_sys::Reflect::set(&highlight, &"kind".into(), &1u32.into()).unwrap(); // Text kind
                            
                            highlights.push(&highlight);
                        }
                        
                        for child in node.children() {
                            find_matching_idents(&child, target, highlights, source);
                        }
                    }
                    
                    find_matching_idents(&root, target_text, &js_highlights, &source);
                    return js_highlights.into();
                }
            }
        }
        
        Array::new().into()
    }
    
    /// Get semantic tokens for the full document
    pub fn semantic_tokens_full(&self, uri: String) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return JsValue::NULL;
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // For WASM, we can provide basic syntax-based semantic tokens
        use typst::syntax::{SyntaxKind, SyntaxNode};
        
        let mut tokens = Vec::new();
        
        fn collect_tokens(node: &SyntaxNode, tokens: &mut Vec<u32>, base_offset: usize) {
            if node.children().count() == 0 {
                // Leaf node - check if it's a token we want to highlight
                let token_type = match node.kind() {
                    SyntaxKind::Ident => 16, // Variable
                    SyntaxKind::Str => 1,    // String
                    SyntaxKind::Int | SyntaxKind::Float => 3, // Number
                    SyntaxKind::LineComment | SyntaxKind::BlockComment => 0, // Comment
                    SyntaxKind::Let | SyntaxKind::Set | SyntaxKind::Show | 
                    SyntaxKind::Import | SyntaxKind::Include | SyntaxKind::If | 
                    SyntaxKind::Else | SyntaxKind::For | SyntaxKind::While => 2, // Keyword
                    SyntaxKind::Hash => 14, // Function/Macro
                    _ => return, // Skip other tokens
                };
                
                let text = node.text();
                let text_len = text.chars().count() as u32;
                
                if text_len > 0 {
                    // Calculate position (simplified - assumes no line breaks in token)
                    let line_delta = 0u32; // Simplified for basic implementation
                    let char_delta = 0u32; // Simplified positioning
                    
                    tokens.extend_from_slice(&[
                        line_delta,
                        char_delta,
                        text_len,
                        token_type,
                        0u32, // Token modifiers
                    ]);
                }
            } else {
                // Recurse into children
                for child in node.children() {
                    collect_tokens(&child, tokens, base_offset);
                }
            }
        }
        
        collect_tokens(source.root(), &mut tokens, 0);
        
        if !tokens.is_empty() {
            let result = Object::new();
            let data_array = js_sys::Uint32Array::new_with_length(tokens.len() as u32);
            
            for (i, &token) in tokens.iter().enumerate() {
                data_array.set_index(i as u32, token);
            }
            
            js_sys::Reflect::set(&result, &"data".into(), &data_array).unwrap();
            result.into()
        } else {
            JsValue::NULL
        }
    }
    
    /// Get semantic tokens delta for the document
    pub fn semantic_tokens_delta(&self, uri: String, previous_result_id: String) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return JsValue::NULL;
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public SemanticTokensDeltaRequest API
        use tinymist_query::{SemanticTokensDeltaRequest, SemanticRequest, PositionEncoding};
        
        let path = std::path::PathBuf::from(&uri);
        
        let request = SemanticTokensDeltaRequest { 
            path,
            previous_result_id 
        };
        
        // For WASM, we don't have a full LocalContext, so we'll return a simple result
        // In a full implementation, we would use request.request(ctx)
        // For now, return null as the method structure is established
        JsValue::NULL
    }
    
    /// Format the document
    pub fn formatting(&self, uri: String) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public FormattingRequest API
        use tinymist_query::FormattingRequest;
        
        let path = std::path::PathBuf::from(&uri);
        let request = FormattingRequest { path };
        
        // For now, return empty array - the actual formatting implementation
        // would need additional infrastructure not available in WASM
        Array::new().into()
    }
    
    /// Get inlay hints for the document in the specified range
    pub fn inlay_hint(&self, uri: String, start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public InlayHintRequest API
        use tinymist_query::{InlayHintRequest, SemanticRequest, PositionEncoding};
        
        let path = std::path::PathBuf::from(&uri);
        let range = lsp_types::Range {
            start: lsp_types::Position { line: start_line, character: start_char },
            end: lsp_types::Position { line: end_line, character: end_char },
        };
        
        let request = InlayHintRequest { path, range };
        
        // For WASM, we don't have a full LocalContext, so we'll return a simple result
        // In a full implementation, we would use request.request(ctx)
        // For now, return empty array as the method structure is established
        Array::new().into()
    }
    
    /// Get document colors
    pub fn document_color(&self, uri: String) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public DocumentColorRequest API
        use tinymist_query::{DocumentColorRequest, SemanticRequest, PositionEncoding};
        
        let path = std::path::PathBuf::from(&uri);
        let request = DocumentColorRequest { path };
        
        // For WASM, we don't have a full LocalContext, so we'll return a simple result
        // In a full implementation, we would use request.request(ctx)
        // For now, return empty array as the method structure is established
        Array::new().into()
    }
    
    /// Get document links
    pub fn document_link(&self, uri: String) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public DocumentLinkRequest API
        use tinymist_query::{DocumentLinkRequest, SemanticRequest, PositionEncoding};
        
        let path = std::path::PathBuf::from(&uri);
        let request = DocumentLinkRequest { path };
        
        // For WASM, we don't have a full LocalContext, so we'll return a simple result
        // In a full implementation, we would use request.request(ctx)
        // For now, return empty array as the method structure is established
        Array::new().into()
    }
    
    /// Get color presentation for a specific color at the specified range
    pub fn color_presentation(&self, uri: String, color: JsValue, start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        // Parse color from JsValue (assuming it's an LSP Color object with r,g,b,a properties)
        let lsp_color = if color.is_object() {
            use lsp_types::Color;
            use wasm_bindgen::JsCast;
            
            let color_obj = color.dyn_into::<js_sys::Object>().unwrap();
            Color {
                red: js_sys::Reflect::get(&color_obj, &"red".into())
                    .ok()
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32)
                    .unwrap_or(0.0),
                green: js_sys::Reflect::get(&color_obj, &"green".into())
                    .ok()
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32)
                    .unwrap_or(0.0),
                blue: js_sys::Reflect::get(&color_obj, &"blue".into())
                    .ok()
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32)
                    .unwrap_or(0.0),
                alpha: js_sys::Reflect::get(&color_obj, &"alpha".into())
                    .ok()
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32)
                    .unwrap_or(1.0),
            }
        } else {
            return Array::new().into();
        };
        
        // Use tinymist-query's public ColorPresentationRequest API
        use tinymist_query::ColorPresentationRequest;
        use lsp_types::{Range, Position};
        
        let path = std::path::PathBuf::from(&uri);
        let range = Range {
            start: Position { line: start_line, character: start_char },
            end: Position { line: end_line, character: end_char },
        };
        
        let request = ColorPresentationRequest { 
            path,
            color: lsp_color,
            range
        };
        
        if let Some(presentations) = request.request() {
            let js_presentations = Array::new();
            
            for presentation in presentations {
                let js_presentation = Object::new();
                
                js_sys::Reflect::set(
                    &js_presentation,
                    &"label".into(),
                    &presentation.label.into()
                ).unwrap();
                
                // Add text_edit if present
                if let Some(text_edit) = presentation.text_edit {
                    let js_text_edit = Object::new();
                    
                    // Add range
                    let js_range = Object::new();
                    let js_start = Object::new();
                    js_sys::Reflect::set(&js_start, &"line".into(), &text_edit.range.start.line.into()).unwrap();
                    js_sys::Reflect::set(&js_start, &"character".into(), &text_edit.range.start.character.into()).unwrap();
                    let js_end = Object::new();
                    js_sys::Reflect::set(&js_end, &"line".into(), &text_edit.range.end.line.into()).unwrap();
                    js_sys::Reflect::set(&js_end, &"character".into(), &text_edit.range.end.character.into()).unwrap();
                    
                    js_sys::Reflect::set(&js_range, &"start".into(), &js_start).unwrap();
                    js_sys::Reflect::set(&js_range, &"end".into(), &js_end).unwrap();
                    
                    js_sys::Reflect::set(&js_text_edit, &"range".into(), &js_range).unwrap();
                    js_sys::Reflect::set(&js_text_edit, &"newText".into(), &text_edit.new_text.into()).unwrap();
                    
                    js_sys::Reflect::set(&js_presentation, &"textEdit".into(), &js_text_edit).unwrap();
                }
                
                js_presentations.push(&js_presentation);
            }
            
            js_presentations.into()
        } else {
            Array::new().into()
        }
    }
    
    /// Get code actions for the specified range
    pub fn code_action(&self, uri: String, start_line: u32, start_char: u32, end_line: u32, end_char: u32, context: JsValue) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public CodeActionRequest API
        use tinymist_query::CodeActionRequest;
        use lsp_types::{Range, Position, CodeActionContext};
        
        let path = std::path::PathBuf::from(&uri);
        let range = Range {
            start: Position { line: start_line, character: start_char },
            end: Position { line: end_line, character: end_char },
        };
        
        // Parse CodeActionContext from JsValue if provided
        let action_context = if context.is_object() {
            use wasm_bindgen::JsCast;
            
            let ctx_obj = context.dyn_into::<js_sys::Object>().unwrap();
            // Extract diagnostics array if present
            let diagnostics = js_sys::Reflect::get(&ctx_obj, &"diagnostics".into())
                .ok()
                .and_then(|v| {
                    if v.is_undefined() || v.is_null() {
                        None
                    } else {
                        Some(vec![]) // For WASM, we'll use empty diagnostics for now
                    }
                })
                .unwrap_or_default();
            
            CodeActionContext {
                diagnostics,
                only: None, // Simplified for WASM
                trigger_kind: None,
            }
        } else {
            CodeActionContext::default()
        };
        
        let request = CodeActionRequest { 
            path,
            range,
            context: action_context
        };
        
        // For WASM, we don't have a full LocalContext, so we'll return a simple result
        // In a full implementation, we would use request.request(ctx)
        // For now, return empty array as the method structure is established
        Array::new().into()
    }
    
    /// Get code lenses for the document
    pub fn code_lens(&self, uri: String) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return Array::new().into();
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public CodeLensRequest API
        use tinymist_query::{CodeLensRequest, SemanticRequest, PositionEncoding};
        
        let path = std::path::PathBuf::from(&uri);
        let request = CodeLensRequest { path };
        
        // For WASM, we don't have a full LocalContext, so we'll return a simple result
        // In a full implementation, we would use request.request(ctx)
        // For now, return empty array as the method structure is established
        Array::new().into()
    }
    
    /// Get signature help at the specified position
    pub fn signature_help(&self, uri: String, line: u32, character: u32) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return JsValue::NULL;
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public SignatureHelpRequest API
        use tinymist_query::{SignatureHelpRequest, SemanticRequest, PositionEncoding, LspPosition};
        
        let path = std::path::PathBuf::from(&uri);
        let lsp_position = LspPosition { line, character };
        
        let request = SignatureHelpRequest { 
            path,
            position: lsp_position
        };
        
        // For WASM, we don't have a full LocalContext, so we'll return a simple result
        // In a full implementation, we would use request.request(ctx)
        // For now, return null as the method structure is established
        JsValue::NULL
    }
    
    /// Rename the symbol at the specified position
    pub fn rename(&self, uri: String, line: u32, character: u32, new_name: String) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return JsValue::NULL;
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public RenameRequest API
        use tinymist_query::{RenameRequest, StatefulRequest, LspPosition};
        
        let path = std::path::PathBuf::from(&uri);
        let lsp_position = LspPosition { line, character };
        
        let request = RenameRequest { 
            path,
            position: lsp_position,
            new_name
        };
        
        // For WASM, we don't have a full LocalContext, so we'll return a simple result
        // In a full implementation, we would use request.request(ctx, graph)
        // For now, return null as the method structure is established
        JsValue::NULL
    }
    
    /// Prepare for rename at the specified position
    pub fn prepare_rename(&self, uri: String, line: u32, character: u32) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return JsValue::NULL;
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public PrepareRenameRequest API
        use tinymist_query::{PrepareRenameRequest, StatefulRequest, LspPosition};
        
        let path = std::path::PathBuf::from(&uri);
        let lsp_position = LspPosition { line, character };
        
        let request = PrepareRenameRequest { 
            path,
            position: lsp_position
        };
        
        // For WASM, we don't have a full LocalContext, so we'll return a simple result
        // In a full implementation, we would use request.request(ctx, graph)
        // For now, return null as the method structure is established
        JsValue::NULL
    }
    
    /// Get workspace symbols matching the pattern
    pub fn symbol(&self, pattern: String) -> JsValue {
        // Use tinymist-query's public SymbolRequest API
        use tinymist_query::{SymbolRequest, SemanticRequest};
        
        let request = SymbolRequest { 
            pattern: if pattern.is_empty() { None } else { Some(pattern) }
        };
        
        // For WASM, we don't have a full LocalContext, so we'll return a simple result
        // In a full implementation, we would use request.request(ctx)
        // For now, return null as the method structure is established
        JsValue::NULL
    }
    
    /// Handle on_enter events
    pub fn on_enter(&self, uri: String, start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> JsValue {
        if !self.documents.contains_key(&uri) {
            return JsValue::NULL;
        }

        let content = &self.documents[&uri];
        
        // Parse the typst source
        let source = typst::syntax::Source::detached(content);
        
        // Use tinymist-query's public OnEnterRequest API
        use tinymist_query::{OnEnterRequest, SyntaxRequest, PositionEncoding};
        use lsp_types::{Position, Range};
        
        let path = std::path::PathBuf::from(&uri);
        
        let range = Range {
            start: Position {
                line: start_line,
                character: start_char,
            },
            end: Position {
                line: end_line,
                character: end_char,
            },
        };
        
        let request = OnEnterRequest { 
            path,
            range
        };
        
        // For WASM, we can actually implement this since it only needs syntax analysis
        if let Some(text_edits) = request.request(&source, PositionEncoding::Utf16) {
            let js_edits = Array::new();
            
            for edit in text_edits {
                let js_edit = Object::new();
                
                // Set range
                let range_obj = Object::new();
                let start_obj = Object::new();
                let end_obj = Object::new();
                
                js_sys::Reflect::set(&start_obj, &"line".into(), &edit.range.start.line.into()).unwrap();
                js_sys::Reflect::set(&start_obj, &"character".into(), &edit.range.start.character.into()).unwrap();
                js_sys::Reflect::set(&end_obj, &"line".into(), &edit.range.end.line.into()).unwrap();
                js_sys::Reflect::set(&end_obj, &"character".into(), &edit.range.end.character.into()).unwrap();
                
                js_sys::Reflect::set(&range_obj, &"start".into(), &start_obj).unwrap();
                js_sys::Reflect::set(&range_obj, &"end".into(), &end_obj).unwrap();
                
                js_sys::Reflect::set(&js_edit, &"range".into(), &range_obj).unwrap();
                js_sys::Reflect::set(&js_edit, &"newText".into(), &edit.new_text.into()).unwrap();
                
                js_edits.push(&js_edit);
            }
            
            return js_edits.into();
        }
        
        JsValue::NULL
    }
    
    /// Handle will_rename_files events
    pub fn will_rename_files(&self, file_renames: JsValue) -> JsValue {
        // For WASM, we can provide basic file rename validation
        // Parse file renames from JsValue
        if file_renames.is_object() {
            use wasm_bindgen::JsCast;
            
            if let Ok(files_array) = file_renames.dyn_into::<js_sys::Array>() {
                let js_edits = Array::new();
                
                // Process each file rename
                for i in 0..files_array.length() {
                    let file_rename = files_array.get(i);
                    if file_rename.is_object() {
                        if let Ok(rename_obj) = file_rename.dyn_into::<js_sys::Object>() {
                            // Extract oldUri and newUri
                            if let (Ok(old_uri), Ok(new_uri)) = (
                                js_sys::Reflect::get(&rename_obj, &"oldUri".into()),
                                js_sys::Reflect::get(&rename_obj, &"newUri".into())
                            ) {
                                if let (Some(old_str), Some(new_str)) = (
                                    old_uri.as_string(),
                                    new_uri.as_string()
                                ) {
                                    // For WASM, we can validate and prepare basic file renames
                                    // In a full implementation, this would update imports and references
                                    web_sys::console::log_1(&format!("File rename: {} -> {}", old_str, new_str).into());
                                }
                            }
                        }
                    }
                }
                
                // Return empty edit for now - in a full implementation this would return
                // workspace edits to update imports and references
                return js_edits.into();
            }
        }
        
        JsValue::NULL
    }
    
    // Helper methods
    
    /// Convert a DocumentSymbol to a JavaScript object
    fn document_symbol_to_js(&self, symbol: &DocumentSymbol) -> Option<Object> {
        let js_symbol = Object::new();
        
        // Set the name
        js_sys::Reflect::set(&js_symbol, &"name".into(), &symbol.name.clone().into()).ok()?;
        
        // Set the kind (using LSP SymbolKind numbers)
        let kind_num: u32 = match symbol.kind {
            lsp_types::SymbolKind::FILE => 1,
            lsp_types::SymbolKind::MODULE => 2,
            lsp_types::SymbolKind::NAMESPACE => 3,
            lsp_types::SymbolKind::PACKAGE => 4,
            lsp_types::SymbolKind::CLASS => 5,
            lsp_types::SymbolKind::METHOD => 6,
            lsp_types::SymbolKind::PROPERTY => 7,
            lsp_types::SymbolKind::FIELD => 8,
            lsp_types::SymbolKind::CONSTRUCTOR => 9,
            lsp_types::SymbolKind::ENUM => 10,
            lsp_types::SymbolKind::INTERFACE => 11,
            lsp_types::SymbolKind::FUNCTION => 12,
            lsp_types::SymbolKind::VARIABLE => 13,
            lsp_types::SymbolKind::CONSTANT => 14,
            lsp_types::SymbolKind::STRING => 15,
            lsp_types::SymbolKind::NUMBER => 16,
            lsp_types::SymbolKind::BOOLEAN => 17,
            lsp_types::SymbolKind::ARRAY => 18,
            lsp_types::SymbolKind::OBJECT => 19,
            lsp_types::SymbolKind::KEY => 20,
            lsp_types::SymbolKind::NULL => 21,
            lsp_types::SymbolKind::ENUM_MEMBER => 22,
            lsp_types::SymbolKind::STRUCT => 23,
            lsp_types::SymbolKind::EVENT => 24,
            lsp_types::SymbolKind::OPERATOR => 25,
            lsp_types::SymbolKind::TYPE_PARAMETER => 26,
            _ => 13, // Default to VARIABLE for unknown kinds
        };
        js_sys::Reflect::set(&js_symbol, &"kind".into(), &kind_num.into()).ok()?;
        
        // Set the range
        let range_obj = Object::new();
        let start_obj = Object::new();
        let end_obj = Object::new();
        
        js_sys::Reflect::set(&start_obj, &"line".into(), &symbol.range.start.line.into()).ok()?;
        js_sys::Reflect::set(&start_obj, &"character".into(), &symbol.range.start.character.into()).ok()?;
        js_sys::Reflect::set(&end_obj, &"line".into(), &symbol.range.end.line.into()).ok()?;
        js_sys::Reflect::set(&end_obj, &"character".into(), &symbol.range.end.character.into()).ok()?;
        
        js_sys::Reflect::set(&range_obj, &"start".into(), &start_obj).ok()?;
        js_sys::Reflect::set(&range_obj, &"end".into(), &end_obj).ok()?;
        
        js_sys::Reflect::set(&js_symbol, &"range".into(), &range_obj).ok()?;
        js_sys::Reflect::set(&js_symbol, &"selectionRange".into(), &range_obj).ok()?;
        
        // Set detail if available
        if let Some(detail) = &symbol.detail {
            js_sys::Reflect::set(&js_symbol, &"detail".into(), &detail.clone().into()).ok()?;
        }
        
        // Set children if available
        if let Some(children) = &symbol.children {
            let js_children = Array::new();
            for child in children {
                if let Some(child_obj) = self.document_symbol_to_js(child) {
                    js_children.push(&child_obj);
                }
            }
            js_sys::Reflect::set(&js_symbol, &"children".into(), &js_children).ok()?;
        }
        
        Some(js_symbol)
    }
    
    /// Convert a SelectionRange to a JavaScript object
    fn selection_range_to_js(&self, range: &lsp_types::SelectionRange) -> Object {
        let js_range = Object::new();
        
        // Set the range
        let range_obj = Object::new();
        let start_obj = Object::new();
        let end_obj = Object::new();
        
        js_sys::Reflect::set(&start_obj, &"line".into(), &range.range.start.line.into()).unwrap();
        js_sys::Reflect::set(&start_obj, &"character".into(), &range.range.start.character.into()).unwrap();
        js_sys::Reflect::set(&end_obj, &"line".into(), &range.range.end.line.into()).unwrap();
        js_sys::Reflect::set(&end_obj, &"character".into(), &range.range.end.character.into()).unwrap();
        
        js_sys::Reflect::set(&range_obj, &"start".into(), &start_obj).unwrap();
        js_sys::Reflect::set(&range_obj, &"end".into(), &end_obj).unwrap();
        js_sys::Reflect::set(&js_range, &"range".into(), &range_obj).unwrap();
        
        // Set parent if available
        if let Some(parent) = &range.parent {
            let parent_obj = self.selection_range_to_js(parent);
            js_sys::Reflect::set(&js_range, &"parent".into(), &parent_obj).unwrap();
        }
        
        js_range
    }
}