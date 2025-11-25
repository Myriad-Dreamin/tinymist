//! Dumps typst knowledge from workspace.
//!
//! Reference Impls:
//! - <https://github.com/sourcegraph/lsif-jsonnet/blob/e186f9fde623efa8735261e9cb059ad3a58b535f/dumper/dumper.go>
//! - <https://github.com/rust-lang/rust-analyzer/blob/5c0b555a65cadc14a6a16865c3e065c9d30b0bef/crates/ide/src/static_index.rs>
//! - <https://github.com/rust-lang/rust-analyzer/blob/5c0b555a65cadc14a6a16865c3e065c9d30b0bef/crates/rust-analyzer/src/cli/lsif.rs>

use core::fmt;
use std::sync::Arc;

use crate::analysis::{SemanticTokens, SharedContext};
use crate::index::protocol::ResultSet;
use crate::prelude::Definition;
use crate::{LocalContext, path_to_url};
use ecow::EcoString;
use lsp_types::Url;
use tinymist_analysis::syntax::classify_syntax;
use tinymist_std::error::WithContextUntyped;
use tinymist_std::hash::FxHashMap;
use tinymist_world::EntryReader;
use typst::syntax::{FileId, LinkedNode, Source, Span};

pub mod protocol;
use protocol as p;

/// The dumped knowledge.
pub struct Knowledge {
    /// The meta data.
    pub meta: p::MetaData,
    /// The files.
    pub files: Vec<FileIndex>,
}

impl Knowledge {
    /// Creates a new empty knowledge.
    pub fn bind<'a>(&'a self, ctx: &'a Arc<SharedContext>) -> KnowledgeWithContext<'a> {
        KnowledgeWithContext {
            knowledge: self,
            ctx,
        }
    }
}

/// A view of knowledge with context for dumping.
pub struct KnowledgeWithContext<'a> {
    knowledge: &'a Knowledge,
    ctx: &'a Arc<SharedContext>,
}

impl fmt::Display for KnowledgeWithContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut files = FxHashMap::default();
        let mut encoder = LsifEncoder {
            ctx: self.ctx,
            writer: f,
            id: IdCounter::new(),
            files: &mut files,
            results: FxHashMap::default(),
        };

        encoder.emit_meta(&self.knowledge.meta).map_err(|err| {
            log::error!("cannot write meta data: {err}");
            fmt::Error
        })?;
        encoder.emit_files(&self.knowledge.files).map_err(|err| {
            log::error!("cannot write files: {err}");
            fmt::Error
        })
    }
}

struct IdCounter {
    next: usize,
}

impl IdCounter {
    fn new() -> Self {
        Self { next: 0 }
    }

    fn next(&mut self) -> usize {
        let id = self.next;
        self.next += 1;
        id
    }
}

trait LsifWrite {
    fn write_element(&mut self, id: i32, element: p::Element) -> fmt::Result;
}

impl<T: fmt::Write> LsifWrite for T {
    fn write_element(&mut self, id: i32, element: p::Element) -> fmt::Result {
        let entry = p::Entry { id, data: element };
        self.write_str(&serde_json::to_string(&entry).unwrap())?;
        self.write_char('\n')
    }
}

struct LsifEncoder<'a, W: fmt::Write> {
    ctx: &'a Arc<SharedContext>,
    writer: &'a mut W,
    id: IdCounter,
    files: &'a mut FxHashMap<FileId, i32>,
    results: FxHashMap<Span, i32>,
}

impl<'a, W: fmt::Write> LsifEncoder<'a, W> {
    fn alloc_file_id(&mut self, fid: FileId) -> i32 {
        *self.files.entry(fid).or_insert_with(|| {
            let id = self.id.next() as i32;
            self.writer
                .write_element(
                    id,
                    p::Element::Vertex(p::Vertex::Document(&p::Document {
                        uri: self.ctx.uri_for_id(fid).unwrap_or_else(|err| {
                            log::error!("cannot get uri for {fid:?}: {err}");
                            Url::parse("file:///unknown").unwrap()
                        }),
                        language_id: EcoString::inline("typst"),
                    })),
                )
                .unwrap();

            id
        })
    }

    fn alloc_result_id(&mut self, span: Span) -> tinymist_std::Result<i32> {
        if let Some(id) = self.results.get(&span) {
            return Ok(*id);
        }

        let id = self.emit_element(p::Element::Vertex(p::Vertex::ResultSet(ResultSet {
            key: None,
        })))?;

        self.results.insert(span, id);
        Ok(id)
    }

    fn emit_element(&mut self, element: p::Element) -> tinymist_std::Result<i32> {
        let id = self.id.next() as i32;
        self.writer
            .write_element(id, element)
            .context_ut("cannot write element")?;
        Ok(id)
    }

    fn emit_meta(&mut self, meta: &p::MetaData) -> tinymist_std::Result<()> {
        let obj = p::Element::Vertex(p::Vertex::MetaData(meta));
        self.emit_element(obj).map(|_| ())
    }

    fn emit_files(&mut self, files: &[FileIndex]) -> tinymist_std::Result<()> {
        for (idx, file) in files.iter().enumerate() {
            eprintln!("emit file: {:?}, {idx} of {}", file.fid, files.len());
            let source = self
                .ctx
                .source_by_id(file.fid)
                .context_ut("cannot get source")?;
            let fid = self.alloc_file_id(file.fid);
            let semantic_tokens_id =
                self.emit_element(p::Element::Vertex(p::Vertex::SemanticTokensResult {
                    result: lsp_types::SemanticTokens {
                        result_id: None,
                        data: file.semantic_tokens.as_ref().clone(),
                    },
                }))?;
            self.emit_element(p::Element::Edge(p::Edge::SemanticTokens(p::EdgeData {
                out_v: fid,
                in_v: semantic_tokens_id,
            })))?;

            let tokens_id = file
                .references
                .iter()
                .flat_map(|(k, v)| {
                    let rng = self.emit_span(*k, &source);
                    let def_rng = self.emit_def_span(v, &source, false);
                    rng.into_iter().chain(def_rng.into_iter())
                })
                .collect();
            self.emit_element(p::Element::Edge(p::Edge::Contains(p::EdgeDataMultiIn {
                out_v: fid,
                in_vs: tokens_id,
            })))?;

            for (s, def) in &file.references {
                let res_id = self.alloc_result_id(*s)?;
                self.emit_element(p::Element::Edge(p::Edge::Next(p::EdgeData {
                    out_v: res_id,
                    in_v: fid,
                })))?;
                let def_id = self.emit_element(p::Element::Vertex(p::Vertex::DefinitionResult))?;
                let Some(def_range) = self.emit_def_span(def, &source, true) else {
                    continue;
                };
                let Some(file_id) = def.file_id() else {
                    continue;
                };
                let file_vertex_id = self.alloc_file_id(file_id);

                self.emit_element(p::Element::Edge(p::Edge::Item(p::Item {
                    document: file_vertex_id,
                    property: None,
                    edge_data: p::EdgeDataMultiIn {
                        in_vs: vec![def_range],
                        out_v: def_id,
                    },
                })))?;
                self.emit_element(p::Element::Edge(p::Edge::Definition(p::EdgeData {
                    in_v: def_id,
                    out_v: res_id,
                })))?;
            }
        }
        Ok(())
    }

    fn emit_span(&mut self, span: Span, source: &Source) -> Option<i32> {
        let range = source.range(span)?;
        self.emit_element(p::Element::Vertex(p::Vertex::Range {
            range: self.ctx.to_lsp_range(range, source),
            tag: None,
        }))
        .ok()
    }

    fn emit_def_span(&mut self, def: &Definition, source: &Source, external: bool) -> Option<i32> {
        let s = def.decl.span();
        if !s.is_detached() && s.id() == Some(source.id()) {
            self.emit_span(s, source)
        } else if let Some(fid) = def.file_id()
            && fid == source.id()
        {
            // todo: module it self
            None
        } else if external && !s.is_detached() {
            let external_src = self.ctx.source_by_id(def.file_id()?).ok()?;
            self.emit_span(s, &external_src)
        } else {
            None
        }
    }
}

/// The index of a file.
pub struct FileIndex {
    /// The file id.
    pub fid: FileId,
    /// The semantic tokens of the file.
    pub semantic_tokens: SemanticTokens,
    /// The documentation of the file.
    pub documentation: Option<EcoString>,
    /// The references in the file.
    pub references: FxHashMap<Span, Definition>,
}

/// Dumps typst knowledge in [LSIF] format from workspace.
///
/// [LSIF]: https://microsoft.github.io/language-server-protocol/specifications/lsif/0.6.0/specification/
pub fn knowledge(ctx: &mut LocalContext) -> tinymist_std::Result<Knowledge> {
    let root = ctx
        .world()
        .entry_state()
        .workspace_root()
        .ok_or_else(|| tinymist_std::error_once!("workspace root is not set"))?;

    let files = ctx.source_files().clone();

    let mut worker = DumpWorker {
        ctx,
        strings: FxHashMap::default(),
        references: FxHashMap::default(),
    };
    let files = files
        .iter()
        .map(move |fid| worker.file(fid))
        .collect::<tinymist_std::Result<Vec<FileIndex>>>()?;

    Ok(Knowledge {
        meta: p::MetaData {
            version: "0.6.0".to_string(),
            project_root: path_to_url(&root)?,
            position_encoding: p::Encoding::Utf16,
            tool_info: Some(p::ToolInfo {
                name: "tinymist".to_string(),
                args: vec![],
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        },
        files,
    })
}

struct DumpWorker<'a> {
    /// The context.
    ctx: &'a mut LocalContext,
    /// A string interner.
    strings: FxHashMap<EcoString, EcoString>,
    /// The references collected so far.
    references: FxHashMap<Span, Definition>,
}

impl DumpWorker<'_> {
    fn file(&mut self, fid: &FileId) -> tinymist_std::Result<FileIndex> {
        let source = self.ctx.source_by_id(*fid).context_ut("cannot parse")?;
        let semantic_tokens = crate::SemanticTokensFullRequest::compute(self.ctx, &source);

        let root = LinkedNode::new(source.root());
        self.walk(&source, &root);
        let references = std::mem::take(&mut self.references);

        Ok(FileIndex {
            fid: *fid,
            semantic_tokens,
            documentation: Some(self.intern("File documentation.")), // todo
            references,
        })
    }

    fn intern(&mut self, s: &str) -> EcoString {
        if let Some(v) = self.strings.get(s) {
            return v.clone();
        }
        let v = EcoString::from(s);
        self.strings.insert(v.clone(), v.clone());
        v
    }

    fn walk(&mut self, source: &Source, node: &LinkedNode) {
        if node.get().children().len() == 0 {
            let Some(syntax) = classify_syntax(node.clone(), node.offset()) else {
                return;
            };
            let span = syntax.node().span();
            if self.references.contains_key(&span) {
                return;
            }

            let Some(def) = self.ctx.def_of_syntax(source, syntax) else {
                return;
            };
            self.references.insert(span, def);

            return;
        }

        for child in node.children() {
            self.walk(source, &child);
        }
    }
}
