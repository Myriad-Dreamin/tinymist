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
                        if let Some(t) = leaf {
                            match t.kind() {
                                SyntaxKind::LineComment | SyntaxKind::BlockComment => {
                                    break InterpretMode::Comment
                                }
                                SyntaxKind::Raw => break InterpretMode::Raw,
                                SyntaxKind::Str => break InterpretMode::String,
                                SyntaxKind::Markup => break InterpretMode::Markup,
                                SyntaxKind::FuncCall
                                | SyntaxKind::Unary
                                | SyntaxKind::Binary
                                | SyntaxKind::Parenthesized
                                | SyntaxKind::Dict
                                | SyntaxKind::Array
                                | SyntaxKind::Code => break InterpretMode::Code,
                                SyntaxKind::Equation | SyntaxKind::Math => {
                                    break InterpretMode::Math
                                }
                                SyntaxKind::Text => break InterpretMode::Markup,
                                SyntaxKind::Space => {}
                                SyntaxKind::Linebreak => {}
                                SyntaxKind::Parbreak => {}
                                SyntaxKind::Escape => {}
                                SyntaxKind::Shorthand => {}
                                SyntaxKind::SmartQuote => {}
                                SyntaxKind::Strong => break InterpretMode::Markup,
                                SyntaxKind::Emph => break InterpretMode::Markup,
                                SyntaxKind::RawLang => {}
                                SyntaxKind::RawDelim => {}
                                SyntaxKind::RawTrimmed => {}
                                SyntaxKind::Link => break InterpretMode::Markup,
                                SyntaxKind::Label => break InterpretMode::Markup,
                                SyntaxKind::Ref => break InterpretMode::Markup,
                                SyntaxKind::RefMarker => break InterpretMode::Markup,
                                SyntaxKind::Heading => break InterpretMode::Markup,
                                SyntaxKind::HeadingMarker => break InterpretMode::Markup,
                                SyntaxKind::ListItem => break InterpretMode::Markup,
                                SyntaxKind::ListMarker => break InterpretMode::Markup,
                                SyntaxKind::EnumItem => break InterpretMode::Markup,
                                SyntaxKind::EnumMarker => break InterpretMode::Markup,
                                SyntaxKind::TermItem => break InterpretMode::Markup,
                                SyntaxKind::TermMarker => break InterpretMode::Markup,
                                SyntaxKind::MathIdent => break InterpretMode::Math,
                                SyntaxKind::MathAlignPoint => break InterpretMode::Math,
                                SyntaxKind::MathDelimited => break InterpretMode::Math,
                                SyntaxKind::MathAttach => break InterpretMode::Math,
                                SyntaxKind::MathPrimes => break InterpretMode::Math,
                                SyntaxKind::MathFrac => break InterpretMode::Math,
                                SyntaxKind::MathRoot => break InterpretMode::Math,
                                SyntaxKind::Hash => {}
                                SyntaxKind::LeftBrace => {}
                                SyntaxKind::RightBrace => {}
                                SyntaxKind::LeftBracket => {}
                                SyntaxKind::RightBracket => {}
                                SyntaxKind::LeftParen => {}
                                SyntaxKind::RightParen => {}
                                SyntaxKind::Comma => {}
                                SyntaxKind::Semicolon => {}
                                SyntaxKind::Colon => {}
                                SyntaxKind::Star => {}
                                SyntaxKind::Underscore => {}
                                SyntaxKind::Dollar => {}
                                SyntaxKind::Plus => {}
                                SyntaxKind::Minus => {}
                                SyntaxKind::Slash => {}
                                SyntaxKind::Hat => {}
                                SyntaxKind::Prime => {}
                                SyntaxKind::Dot => {}
                                SyntaxKind::Eq => {}
                                SyntaxKind::EqEq => {}
                                SyntaxKind::ExclEq => {}
                                SyntaxKind::Lt => {}
                                SyntaxKind::LtEq => {}
                                SyntaxKind::Gt => {}
                                SyntaxKind::GtEq => {}
                                SyntaxKind::PlusEq => {}
                                SyntaxKind::HyphEq => {}
                                SyntaxKind::StarEq => {}
                                SyntaxKind::SlashEq => {}
                                SyntaxKind::Dots => {}
                                SyntaxKind::Arrow => {}
                                SyntaxKind::Root => {}
                                SyntaxKind::Not => {}
                                SyntaxKind::And => {}
                                SyntaxKind::Or => {}
                                SyntaxKind::None => {}
                                SyntaxKind::Auto => {}
                                SyntaxKind::Let => break InterpretMode::Code,
                                SyntaxKind::Set => break InterpretMode::Code,
                                SyntaxKind::Show => break InterpretMode::Code,
                                SyntaxKind::Context => break InterpretMode::Code,
                                SyntaxKind::If => break InterpretMode::Code,
                                SyntaxKind::Else => break InterpretMode::Code,
                                SyntaxKind::For => break InterpretMode::Code,
                                SyntaxKind::In => break InterpretMode::Code,
                                SyntaxKind::While => break InterpretMode::Code,
                                SyntaxKind::Break => break InterpretMode::Code,
                                SyntaxKind::Continue => break InterpretMode::Code,
                                SyntaxKind::Return => break InterpretMode::Code,
                                SyntaxKind::Import => break InterpretMode::Code,
                                SyntaxKind::Include => break InterpretMode::Code,
                                SyntaxKind::As => {}
                                SyntaxKind::Ident => break InterpretMode::Code,
                                SyntaxKind::Bool => break InterpretMode::Code,
                                SyntaxKind::Int => break InterpretMode::Code,
                                SyntaxKind::Float => break InterpretMode::Code,
                                SyntaxKind::Numeric => break InterpretMode::Code,
                                SyntaxKind::CodeBlock => break InterpretMode::Code,
                                SyntaxKind::ContentBlock => break InterpretMode::Markup,
                                SyntaxKind::Named => {}
                                SyntaxKind::Keyed => {}
                                SyntaxKind::FieldAccess => break InterpretMode::Code,
                                SyntaxKind::Args => break InterpretMode::Code,
                                SyntaxKind::Spread => break InterpretMode::Code,
                                SyntaxKind::Closure => break InterpretMode::Code,
                                SyntaxKind::Params => break InterpretMode::Code,
                                SyntaxKind::LetBinding => break InterpretMode::Code,
                                SyntaxKind::SetRule => break InterpretMode::Code,
                                SyntaxKind::ShowRule => break InterpretMode::Code,
                                SyntaxKind::Contextual => break InterpretMode::Code,
                                SyntaxKind::Conditional => break InterpretMode::Code,
                                SyntaxKind::WhileLoop => break InterpretMode::Code,
                                SyntaxKind::ForLoop => break InterpretMode::Code,
                                SyntaxKind::ModuleImport => break InterpretMode::Code,
                                SyntaxKind::ImportItems => break InterpretMode::Code,
                                SyntaxKind::RenamedImportItem => break InterpretMode::Code,
                                SyntaxKind::ModuleInclude => break InterpretMode::Code,
                                SyntaxKind::LoopBreak => break InterpretMode::Code,
                                SyntaxKind::LoopContinue => break InterpretMode::Code,
                                SyntaxKind::FuncReturn => break InterpretMode::Code,
                                SyntaxKind::Destructuring => break InterpretMode::Code,
                                SyntaxKind::DestructAssignment => break InterpretMode::Code,
                                SyntaxKind::Error => {}
                                SyntaxKind::Eof => {}
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
