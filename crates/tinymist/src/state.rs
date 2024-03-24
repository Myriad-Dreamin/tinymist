//! Bootstrap actors for Tinymist.

use std::path::PathBuf;

use ::typst::{diag::FileResult, syntax::Source};
use anyhow::anyhow;
use lsp_types::TextDocumentContentChangeEvent;
use tinymist_query::{
    lsp_to_typst, CompilerQueryRequest, CompilerQueryResponse, PositionEncoding, SyntaxRequest,
};
use typst_ts_compiler::{
    vfs::notify::{FileChangeSet, MemoryEvent},
    Time,
};
use typst_ts_core::{error::prelude::*, Bytes, Error, ImmutPath};

use crate::TypstLanguageServer;

#[derive(Debug, Clone)]
pub struct MemoryFileMeta {
    pub mt: Time,
    pub content: Source,
}

impl TypstLanguageServer {
    /// Updates the main entry
    // todo: the changed entry may be out of root directory
    pub fn update_main_entry(&mut self, new_entry: Option<ImmutPath>) -> Result<(), Error> {
        self.pinning = new_entry.is_some();
        match (new_entry, self.main.is_some()) {
            (Some(new_entry), true) => {
                let main = self.main.as_mut().unwrap();
                main.change_entry(Some(new_entry), |e| self.config.determine_root(e.as_ref()))?;
            }
            (Some(new_entry), false) => {
                let main_node = self.server("main".to_owned(), Some(new_entry));

                self.main = Some(main_node);
            }
            (None, true) => {
                let main = self.main.take().unwrap();
                std::thread::spawn(move || main.settle());
            }
            (None, false) => {}
        };

        Ok(())
    }

    /// Updates the primary (focusing) entry
    pub fn update_primary_entry(&self, new_entry: Option<ImmutPath>) -> Result<(), Error> {
        self.primary().change_entry(new_entry.clone(), |e| {
            self.config.determine_root(e.as_ref())
        })?;

        Ok(())
    }
}

impl TypstLanguageServer {
    fn update_source(&self, files: FileChangeSet) -> Result<(), Error> {
        let main = self.main.as_ref();
        let primary = Some(self.primary());
        let clients_to_notify = (primary.into_iter()).chain(main);

        for client in clients_to_notify {
            client.add_memory_changes(MemoryEvent::Update(files.clone()), |e| {
                self.config.determine_root(e.as_ref())
            });
        }

        Ok(())
    }

    pub fn create_source(&mut self, path: PathBuf, content: String) -> Result<(), Error> {
        let now = Time::now();
        let path: ImmutPath = path.into();

        self.memory_changes.insert(
            path.clone(),
            MemoryFileMeta {
                mt: now,
                content: Source::detached(content.clone()),
            },
        );

        let content: Bytes = content.as_bytes().into();
        log::info!("create source: {:?}", path);

        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_inserts(vec![(path, FileResult::Ok((now, content)).into())]);

        self.update_source(files)
    }

    pub fn remove_source(&mut self, path: PathBuf) -> Result<(), Error> {
        let path: ImmutPath = path.into();

        self.memory_changes.remove(&path);
        log::info!("remove source: {:?}", path);

        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_removes(vec![path]);

        self.update_source(files)
    }

    pub fn edit_source(
        &mut self,
        path: PathBuf,
        content: Vec<TextDocumentContentChangeEvent>,
        position_encoding: PositionEncoding,
    ) -> Result<(), Error> {
        let now = Time::now();
        let path: ImmutPath = path.into();

        let meta = self
            .memory_changes
            .get_mut(&path)
            .ok_or_else(|| error_once!("file missing", path: path.display()))?;

        for change in content {
            let replacement = change.text;
            match change.range {
                Some(lsp_range) => {
                    let range = lsp_to_typst::range(lsp_range, position_encoding, &meta.content)
                        .expect("invalid range");
                    meta.content.edit(range, &replacement);
                }
                None => {
                    meta.content.replace(&replacement);
                }
            }
        }

        meta.mt = now;

        let snapshot = FileResult::Ok((now, meta.content.text().as_bytes().into())).into();

        let files = FileChangeSet::new_inserts(vec![(path.clone(), snapshot)]);

        self.update_source(files)
    }
}

#[macro_export]
macro_rules! run_query {
    ($self: ident.$query: ident ($($arg_key:ident),+ $(,)?)) => {{
        use tinymist_query::*;
        let req = paste! { [<$query Request>] { $($arg_key),+ } };
        $self
            .query(CompilerQueryRequest::$query(req.clone()))
            .map_err(|err| {
                error!("error getting $query: {err} with request {req:?}");
                internal_error("Internal error")
            })
            .map(|resp| {
                let CompilerQueryResponse::$query(resp) = resp else {
                    unreachable!()
                };
                resp
            })
    }};
}

macro_rules! query_source {
    ($self:ident, $method:ident, $req:expr) => {{
        let path: ImmutPath = $req.path.clone().into();
        let snapshot = $self.memory_changes.get(&path);
        let snapshot = snapshot.ok_or_else(|| anyhow!("file missing {:?}", path))?;
        let source = snapshot.content.clone();

        let enc = $self.const_config.position_encoding;
        let res = $req.request(&source, enc);
        Ok(CompilerQueryResponse::$method(res))
    }};
}

macro_rules! query_tokens_cache {
    ($self:ident, $method:ident, $req:expr) => {{
        let path: ImmutPath = $req.path.clone().into();
        let snapshot = $self.memory_changes.get(&path);
        let snapshot = snapshot.ok_or_else(|| anyhow!("file missing {:?}", path))?;
        let source = snapshot.content.clone();

        let res = $req.request(&$self.tokens_ctx, source);
        Ok(CompilerQueryResponse::$method(res))
    }};
}

impl TypstLanguageServer {
    pub fn query(&self, query: CompilerQueryRequest) -> anyhow::Result<CompilerQueryResponse> {
        use CompilerQueryRequest::*;

        match query {
            SemanticTokensFull(req) => query_tokens_cache!(self, SemanticTokensFull, req),
            SemanticTokensDelta(req) => query_tokens_cache!(self, SemanticTokensDelta, req),
            FoldingRange(req) => query_source!(self, FoldingRange, req),
            SelectionRange(req) => query_source!(self, SelectionRange, req),
            DocumentSymbol(req) => query_source!(self, DocumentSymbol, req),
            _ => {
                let query_target = match self.main.as_ref() {
                    Some(main) if self.pinning => main,
                    Some(..) | None => {
                        // todo: race condition, we need atomic primary query
                        if let Some(path) = query.associated_path() {
                            self.primary().change_entry(Some(path.into()), |e| {
                                self.config.determine_root(e.as_ref())
                            })?;
                        }
                        self.primary()
                    }
                };

                query_target.query(query)
            }
        }
    }
}
