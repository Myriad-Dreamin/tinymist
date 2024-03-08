pub mod analysis;

pub(crate) mod diagnostics;

pub use diagnostics::*;
pub(crate) mod signature_help;
pub use signature_help::*;
pub(crate) mod document_symbol;
pub use document_symbol::*;
pub(crate) mod symbol;
pub use symbol::*;
pub(crate) mod semantic_tokens;
pub use semantic_tokens::*;
pub(crate) mod semantic_tokens_full;
pub use semantic_tokens_full::*;
pub(crate) mod semantic_tokens_delta;
pub use semantic_tokens_delta::*;
pub(crate) mod hover;
pub use hover::*;
pub(crate) mod completion;
pub use completion::*;
pub(crate) mod folding_range;
pub use folding_range::*;
pub(crate) mod selection_range;
pub use selection_range::*;
pub(crate) mod goto_definition;
pub use goto_definition::*;
pub(crate) mod inlay_hint;
pub use inlay_hint::*;
pub(crate) mod prepare_rename;
pub use prepare_rename::*;
pub(crate) mod rename;
pub use rename::*;

pub mod lsp_typst_boundary;
pub use lsp_typst_boundary::*;

mod prelude;

mod polymorphic {
    use super::prelude::*;
    use super::*;

    #[derive(Debug, Clone)]
    pub struct OnSaveExportRequest {
        pub path: PathBuf,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum FoldRequestFeature {
        PinnedFirst,
        Unique,
        Mergable,
        ContextFreeUnique,
    }

    #[derive(Debug, Clone)]
    pub enum CompilerQueryRequest {
        OnSaveExport(OnSaveExportRequest),
        Hover(HoverRequest),
        GotoDefinition(GotoDefinitionRequest),
        InlayHint(InlayHintRequest),
        Completion(CompletionRequest),
        SignatureHelp(SignatureHelpRequest),
        Rename(RenameRequest),
        PrepareRename(PrepareRenameRequest),
        DocumentSymbol(DocumentSymbolRequest),
        Symbol(SymbolRequest),
        SemanticTokensFull(SemanticTokensFullRequest),
        SemanticTokensDelta(SemanticTokensDeltaRequest),
        FoldingRange(FoldingRangeRequest),
        SelectionRange(SelectionRangeRequest),
    }

    impl CompilerQueryRequest {
        pub fn fold_feature(&self) -> FoldRequestFeature {
            use FoldRequestFeature::*;
            match self {
                CompilerQueryRequest::OnSaveExport(..) => Mergable,
                CompilerQueryRequest::Hover(..) => PinnedFirst,
                CompilerQueryRequest::GotoDefinition(..) => PinnedFirst,
                CompilerQueryRequest::InlayHint(..) => Unique,
                CompilerQueryRequest::Completion(..) => Mergable,
                CompilerQueryRequest::SignatureHelp(..) => PinnedFirst,
                CompilerQueryRequest::Rename(..) => Mergable,
                CompilerQueryRequest::PrepareRename(..) => Mergable,
                CompilerQueryRequest::DocumentSymbol(..) => ContextFreeUnique,
                CompilerQueryRequest::Symbol(..) => Mergable,
                CompilerQueryRequest::SemanticTokensFull(..) => ContextFreeUnique,
                CompilerQueryRequest::SemanticTokensDelta(..) => ContextFreeUnique,
                CompilerQueryRequest::FoldingRange(..) => ContextFreeUnique,
                CompilerQueryRequest::SelectionRange(..) => ContextFreeUnique,
            }
        }

        pub fn associated_path(&self) -> Option<&Path> {
            Some(match self {
                CompilerQueryRequest::OnSaveExport(req) => &req.path,
                CompilerQueryRequest::Hover(req) => &req.path,
                CompilerQueryRequest::GotoDefinition(req) => &req.path,
                CompilerQueryRequest::InlayHint(req) => &req.path,
                CompilerQueryRequest::Completion(req) => &req.path,
                CompilerQueryRequest::SignatureHelp(req) => &req.path,
                CompilerQueryRequest::Rename(req) => &req.path,
                CompilerQueryRequest::PrepareRename(req) => &req.path,
                CompilerQueryRequest::DocumentSymbol(req) => &req.path,
                CompilerQueryRequest::Symbol(..) => return None,
                CompilerQueryRequest::SemanticTokensFull(req) => &req.path,
                CompilerQueryRequest::SemanticTokensDelta(req) => &req.path,
                CompilerQueryRequest::FoldingRange(req) => &req.path,
                CompilerQueryRequest::SelectionRange(req) => &req.path,
            })
        }
    }

    #[derive(Debug, Clone)]
    pub enum CompilerQueryResponse {
        OnSaveExport(()),
        Hover(Option<Hover>),
        GotoDefinition(Option<GotoDefinitionResponse>),
        InlayHint(Option<Vec<InlayHint>>),
        Completion(Option<CompletionResponse>),
        SignatureHelp(Option<SignatureHelp>),
        PrepareRename(Option<PrepareRenameResponse>),
        Rename(Option<WorkspaceEdit>),
        DocumentSymbol(Option<DocumentSymbolResponse>),
        Symbol(Option<Vec<SymbolInformation>>),
        SemanticTokensFull(Option<SemanticTokensResult>),
        SemanticTokensDelta(Option<SemanticTokensFullDeltaResult>),
        FoldingRange(Option<Vec<FoldingRange>>),
        SelectionRange(Option<Vec<SelectionRange>>),
    }
}

pub use polymorphic::*;

#[cfg(test)]
mod tests {
    use core::fmt;
    use std::{
        collections::HashSet,
        path::{Path, PathBuf},
    };

    use once_cell::sync::Lazy;
    use serde::Serialize;
    use serde_json::{ser::PrettyFormatter, Serializer, Value};
    use typst_ts_compiler::ShadowApiExt;
    use typst_ts_core::{config::CompileOpts, Bytes};

    pub use insta::assert_snapshot;
    pub use typst_ts_compiler::TypstSystemWorld;

    pub fn run_with_source<T>(
        source: &str,
        f: impl FnOnce(&mut TypstSystemWorld, PathBuf) -> T,
    ) -> T {
        let root = if cfg!(windows) {
            PathBuf::from("C:\\")
        } else {
            PathBuf::from("/")
        };
        let mut world = TypstSystemWorld::new(CompileOpts {
            root_dir: root.clone(),
            ..Default::default()
        })
        .unwrap();
        let pw = &root.join(Path::new("/main.typ"));
        world
            .with_shadow_file(pw, Bytes::from(source.as_bytes()), move |e| {
                Ok(f(e, pw.to_owned()))
            })
            .unwrap()
    }

    // pub static REDACT_URI: Lazy<RedactFields> = Lazy::new(||
    // RedactFields::from_iter(["uri"]));
    pub static REDACT_LOC: Lazy<RedactFields> =
        Lazy::new(|| RedactFields::from_iter(["location", "range", "selectionRange"]));

    pub struct JsonRepr(Value);

    impl JsonRepr {
        pub fn new_pure(v: impl serde::Serialize) -> Self {
            let s = serde_json::to_value(v).unwrap();
            Self(s)
        }

        // pub fn new(v: impl serde::Serialize) -> Self {
        //     let s = serde_json::to_value(v).unwrap();
        //     Self(REDACT_URI.redact(s))
        // }

        pub fn new_redacted(v: impl serde::Serialize, rm: &RedactFields) -> Self {
            let s = serde_json::to_value(v).unwrap();
            Self(rm.redact(s))
        }
    }

    impl fmt::Display for JsonRepr {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let w = std::io::BufWriter::new(Vec::new());
            let mut ser = Serializer::with_formatter(w, PrettyFormatter::with_indent(b" "));
            self.0.serialize(&mut ser).unwrap();

            f.write_str(&String::from_utf8(ser.into_inner().into_inner().unwrap()).unwrap())
        }
    }

    pub trait Redact {
        fn redact(&self, v: Value) -> Value;
    }

    pub struct RedactFields(HashSet<&'static str>);

    impl FromIterator<&'static str> for RedactFields {
        fn from_iter<T: IntoIterator<Item = &'static str>>(iter: T) -> Self {
            Self(iter.into_iter().collect())
        }
    }

    impl Redact for RedactFields {
        fn redact(&self, v: Value) -> Value {
            match v {
                Value::Object(mut m) => {
                    for (_, v) in m.iter_mut() {
                        *v = self.redact(v.clone());
                    }
                    for k in self.0.iter() {
                        m.remove(*k);
                    }
                    Value::Object(m)
                }
                Value::Array(mut a) => {
                    for v in a.iter_mut() {
                        *v = self.redact(v.clone());
                    }
                    Value::Array(a)
                }
                Value::String(s) => Value::String(s),
                v => v,
            }
        }
    }
}
