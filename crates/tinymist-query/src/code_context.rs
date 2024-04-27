use serde::{Deserialize, Serialize};

use crate::{prelude::*, SyntaxRequest};

/// A mode in which a text document is interpreted.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InterpretMode {
    /// The position is in a comment.
    Comment,
    /// The position is in a string.
    String,
    /// The position is in a raw.
    Raw,
    /// The position is in a markup block.
    Markup,
    /// The position is in a code block.
    Code,
    /// The position is in a math equation.
    Math,
}

/// A query to get the mode at a specific position in a text document.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InteractCodeContextQuery {
    /// Get the mode at a specific position in a text document.
    ModeAt {
        /// The position inside the text document.
        position: LspPosition,
    },
}

/// A response to a `InteractCodeContextQuery`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InteractCodeContextResponse {
    /// The mode at the requested position.
    ModeAt {
        /// The mode at the requested position.
        mode: InterpretMode,
    },
}

/// A request to get the mode at a specific position in a text document.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind")]
pub struct InteractCodeContextRequest {
    /// The path to the text document.
    pub path: PathBuf,
    /// The queries to execute.
    pub query: Vec<InteractCodeContextQuery>,
}

impl SyntaxRequest for InteractCodeContextRequest {
    type Response = Vec<InteractCodeContextResponse>;

    fn request(
        self,
        source: &Source,
        positing_encoding: PositionEncoding,
    ) -> Option<Self::Response> {
        let mut responses = Vec::new();

        for query in self.query {
            match query {
                InteractCodeContextQuery::ModeAt { position } => {
                    let pos = lsp_to_typst::position(position, positing_encoding, source)?;

                    // get mode
                    let root = LinkedNode::new(source.root());
                    let leaf = root.leaf_at(pos);
                    let mut leaf = leaf.as_ref();
                    let mode = loop {
                        log::info!("leaf for context: {:?}", leaf);
                        use SyntaxKind::*;
                        if let Some(t) = leaf {
                            match t.kind() {
                                LineComment | BlockComment => break InterpretMode::Comment,
                                Raw => break InterpretMode::Raw,
                                Str => break InterpretMode::String,
                                CodeBlock | Code => break InterpretMode::Code,
                                ContentBlock | Markup => break InterpretMode::Markup,
                                Equation | Math => break InterpretMode::Math,
                                Space | Linebreak | Parbreak | Escape | Shorthand | SmartQuote
                                | RawLang | RawDelim | RawTrimmed | Hash | LeftBrace
                                | RightBrace | LeftBracket | RightBracket | LeftParen
                                | RightParen | Comma | Semicolon | Colon | Star | Underscore
                                | Dollar | Plus | Minus | Slash | Hat | Prime | Dot | Eq | EqEq
                                | ExclEq | Lt | LtEq | Gt | GtEq | PlusEq | HyphEq | StarEq
                                | SlashEq | Dots | Arrow | Root | Not | And | Or | None | Auto
                                | As | Named | Keyed | Error | Eof => {}
                                Text | Strong | Emph | Link | Label | Ref | RefMarker | Heading
                                | HeadingMarker | ListItem | ListMarker | EnumItem | EnumMarker
                                | TermItem | TermMarker => break InterpretMode::Markup,
                                MathIdent | MathAlignPoint | MathDelimited | MathAttach
                                | MathPrimes | MathFrac | MathRoot => break InterpretMode::Math,
                                Let | Set | Show | Context | If | Else | For | In | While
                                | Break | Continue | Return | Import | Include | Ident | Bool
                                | Int | Float | Numeric | FieldAccess | Args | Spread | Closure
                                | Params | LetBinding | SetRule | ShowRule | Contextual
                                | Conditional | WhileLoop | ForLoop | ModuleImport
                                | ImportItems | RenamedImportItem | ModuleInclude | LoopBreak
                                | LoopContinue | FuncReturn | FuncCall | Unary | Binary
                                | Parenthesized | Dict | Array | Destructuring
                                | DestructAssignment => break InterpretMode::Code,
                            }
                            leaf = t.parent();
                        } else {
                            break InterpretMode::Markup;
                        }
                    };

                    responses.push(InteractCodeContextResponse::ModeAt { mode });
                }
            }
        }

        Some(responses)
    }
}
