mod adt;
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
pub(crate) mod code_lens;
pub use code_lens::*;

pub mod lsp_typst_boundary;
pub use lsp_typst_boundary::*;

mod prelude;

mod polymorphic {
    use super::prelude::*;
    use super::*;

    #[derive(Debug, Clone)]
    pub struct OnExportRequest {
        pub path: PathBuf,
    }

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
        OnExport(OnExportRequest),
        OnSaveExport(OnSaveExportRequest),
        Hover(HoverRequest),
        GotoDefinition(GotoDefinitionRequest),
        InlayHint(InlayHintRequest),
        CodeLens(CodeLensRequest),
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
                CompilerQueryRequest::OnExport(..) => Mergable,
                CompilerQueryRequest::OnSaveExport(..) => Mergable,
                CompilerQueryRequest::Hover(..) => PinnedFirst,
                CompilerQueryRequest::GotoDefinition(..) => PinnedFirst,
                CompilerQueryRequest::InlayHint(..) => Unique,
                CompilerQueryRequest::CodeLens(..) => Unique,
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
                CompilerQueryRequest::OnExport(..) => return None,
                CompilerQueryRequest::OnSaveExport(req) => &req.path,
                CompilerQueryRequest::Hover(req) => &req.path,
                CompilerQueryRequest::GotoDefinition(req) => &req.path,
                CompilerQueryRequest::InlayHint(req) => &req.path,
                CompilerQueryRequest::CodeLens(req) => &req.path,
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
        OnExport(Option<PathBuf>),
        OnSaveExport(()),
        Hover(Option<Hover>),
        GotoDefinition(Option<GotoDefinitionResponse>),
        InlayHint(Option<Vec<InlayHint>>),
        CodeLens(Option<Vec<CodeLens>>),
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
    use typst::syntax::{LinkedNode, Source, VirtualPath};
    use typst_ts_compiler::{
        service::{CompileDriver, Compiler, WorkspaceProvider},
        ShadowApi,
    };
    use typst_ts_core::{config::CompileOpts, Bytes, TypstFileId};

    pub use insta::assert_snapshot;
    pub use typst_ts_compiler::TypstSystemWorld;

    use crate::{typst_to_lsp, LspPosition, PositionEncoding};

    pub fn snapshot_testing(name: &str, f: &impl Fn(&mut TypstSystemWorld, PathBuf)) {
        let mut settings = insta::Settings::new();
        settings.set_prepend_module_to_snapshot(false);
        settings.set_snapshot_path(format!("fixtures/{name}/snaps"));
        settings.bind(|| {
            let glob_path = format!("fixtures/{name}/*.typ");
            insta::glob!(&glob_path, |path| {
                let contents = std::fs::read_to_string(path).unwrap();

                run_with_sources(&contents, f);
            });
        });
    }

    pub fn run_with_sources<T>(
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
        let sources = source.split("-----");

        let pw = root.join(Path::new("/main.typ"));
        world.map_shadow(&pw, Bytes::from_static(b"")).unwrap();

        let mut last_pw = None;
        for (i, source) in sources.enumerate() {
            // find prelude
            let mut source = source.trim();
            let mut path = None;

            if source.starts_with("//") {
                let first_line = source.lines().next().unwrap();
                source = source.strip_prefix(first_line).unwrap().trim();

                let content = first_line.strip_prefix("//").unwrap().trim();
                path = content.strip_prefix("path:").map(|e| e.trim().to_owned())
            };

            let path = path.unwrap_or_else(|| format!("/s{i}.typ"));

            let pw = root.join(Path::new(&path));
            world
                .map_shadow(&pw, Bytes::from(source.as_bytes()))
                .unwrap();
            last_pw = Some(pw);
        }

        world.set_main_id(TypstFileId::new(None, VirtualPath::new("/main.typ")));
        let mut driver = CompileDriver::new(world);
        let _ = driver.compile(&mut Default::default());

        let pw = last_pw.unwrap();
        driver.world_mut().set_main_id(TypstFileId::new(
            None,
            VirtualPath::new(pw.strip_prefix(root).unwrap()),
        ));
        f(driver.world_mut(), pw)
    }

    pub fn find_test_position(s: &Source) -> LspPosition {
        let re = s.text().find("/* position */").map(|e| (e, true));
        let re = re.or_else(|| s.text().find("/* position after */").zip(Some(false)));
        let (re, prev) = re
            .ok_or_else(|| panic!("No position marker found in source:\n{}", s.text()))
            .unwrap();

        let n = LinkedNode::new(s.root());
        let mut n = n.leaf_at(re + 1).unwrap();

        while n.kind().is_trivia() {
            let m = if prev {
                n.prev_sibling()
            } else {
                n.next_sibling()
            };
            n = m.or_else(|| n.parent().cloned()).unwrap();
        }

        typst_to_lsp::offset_to_position(n.offset() + 1, PositionEncoding::Utf16, s)
    }

    // pub static REDACT_URI: Lazy<RedactFields> = Lazy::new(||
    // RedactFields::from_iter(["uri"]));
    pub static REDACT_LOC: Lazy<RedactFields> = Lazy::new(|| {
        RedactFields::from_iter([
            "location",
            "range",
            "selectionRange",
            "targetRange",
            "targetSelectionRange",
            "originSelectionRange",
            "targetUri",
        ])
    });

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

    fn pos(v: &Value) -> String {
        match v {
            Value::Object(v) => format!("{}:{}", v["line"], v["character"]),
            Value::Number(v) => v.to_string(),
            _ => "<null>".to_owned(),
        }
    }

    impl Redact for RedactFields {
        fn redact(&self, v: Value) -> Value {
            match v {
                Value::Object(mut m) => {
                    for (_, v) in m.iter_mut() {
                        *v = self.redact(v.clone());
                    }
                    for k in self.0.iter().copied() {
                        let Some(t) = m.remove(k) else {
                            continue;
                        };

                        match k {
                            "range"
                            | "selectionRange"
                            | "originSelectionRange"
                            | "targetRange"
                            | "targetSelectionRange" => {
                                m.insert(
                                    k.to_owned(),
                                    format!("{}:{}", pos(&t["start"]), pos(&t["end"])).into(),
                                );
                            }
                            _ => {}
                        }
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
