//! Bootstrap actors for Tinymist.

use std::path::PathBuf;

use anyhow::anyhow;
use lsp_types::TextDocumentContentChangeEvent;
use tinymist_query::{
    lsp_to_typst, CompilerQueryRequest, CompilerQueryResponse, FoldRequestFeature, OnExportRequest,
    OnSaveExportRequest, PositionEncoding, SemanticRequest, StatefulRequest, SyntaxRequest,
};
use typst::{diag::FileResult, syntax::Source};
use typst_ts_compiler::{
    vfs::notify::{FileChangeSet, MemoryEvent},
    Time,
};
use typst_ts_core::{error::prelude::*, Bytes, Error, ImmutPath};

use crate::{actor::typ_client::CompileClientActor, compiler::CompileServer, TypstLanguageServer};

impl CompileServer {
    /// Focus main file to some path.
    pub fn do_change_entry(&mut self, new_entry: Option<ImmutPath>) -> Result<bool, Error> {
        self.compiler
            .as_mut()
            .unwrap()
            .change_entry(new_entry.clone())
    }
}

impl TypstLanguageServer {
    /// Pin the entry to the given path
    pub fn pin_entry(&mut self, new_entry: Option<ImmutPath>) -> Result<(), Error> {
        self.pinning = new_entry.is_some();
        self.primary.do_change_entry(new_entry)?;

        if !self.pinning {
            let fallback = self.config.compile.determine_default_entry_path();
            let fallback = fallback.or_else(|| self.focusing.clone());
            if let Some(e) = fallback {
                self.primary.do_change_entry(Some(e))?;
            }
        }

        Ok(())
    }

    /// Updates the primary (focusing) entry
    pub fn focus_entry(&mut self, new_entry: Option<ImmutPath>) -> Result<bool, Error> {
        if self.pinning || self.config.compile.has_default_entry_path {
            self.focusing = new_entry;
            return Ok(false);
        }

        self.primary.do_change_entry(new_entry.clone())
    }

    /// This is used for tracking activating document status if a client is not
    /// performing any focus command request.
    ///
    /// See https://github.com/microsoft/language-server-protocol/issues/718
    ///
    /// we do want to focus the file implicitly by `textDocument/diagnostic`
    /// (pullDiagnostics mode), as suggested by language-server-protocol#718,
    /// however, this has poor support, e.g. since neovim 0.10.0.
    pub fn implicit_focus_entry(
        &mut self,
        new_entry: impl FnOnce() -> Option<ImmutPath>,
        site: char,
    ) {
        if self.ever_manual_focusing {
            return;
        }
        // didOpen
        match site {
            // foldingRange, hover, semanticTokens
            'f' | 'h' | 't' => {
                self.ever_focusing_by_activities = true;
            }
            // didOpen
            _ => {
                if self.ever_focusing_by_activities {
                    return;
                }
            }
        }

        let new_entry = new_entry();

        let update_result = self.focus_entry(new_entry.clone());
        match update_result {
            Ok(true) => {
                log::info!("file focused[implicit,{site}]: {new_entry:?}");
            }
            Err(err) => {
                log::warn!("could not focus file: {err}");
            }
            Ok(false) => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryFileMeta {
    pub mt: Time,
    pub content: Source,
}

impl TypstLanguageServer {
    fn update_source(&self, files: FileChangeSet) -> Result<(), Error> {
        let primary = Some(self.primary());
        let clients_to_notify =
            (primary.into_iter()).chain(self.dedicates.iter().map(CompileServer::compiler));

        for client in clients_to_notify {
            client.add_memory_changes(MemoryEvent::Update(files.clone()));
        }

        Ok(())
    }

    pub fn create_source(&mut self, path: PathBuf, content: String) -> Result<(), Error> {
        let now = Time::now();
        let path: ImmutPath = path.into();

        self.primary.memory_changes.insert(
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

        self.primary.memory_changes.remove(&path);
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
            .primary
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
    ($self: ident.$query: ident ($($arg_key:ident),* $(,)?)) => {{
        use tinymist_query::*;
        let req = paste::paste! { [<$query Request>] { $($arg_key),* } };
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

        $self.query_source(path, |source| {
            let enc = $self.const_config.position_encoding;
            let res = $req.request(&source, enc);
            Ok(CompilerQueryResponse::$method(res))
        })
    }};
}

macro_rules! query_tokens_cache {
    ($self:ident, $method:ident, $req:expr) => {{
        let path: ImmutPath = $req.path.clone().into();

        $self.query_source(path, |source| {
            let res = $req.request(&$self.tokens_ctx, source);
            Ok(CompilerQueryResponse::$method(res))
        })
    }};
}

macro_rules! query_state {
    ($self:ident, $method:ident, $req:expr) => {{
        let res = $self.steal_state(move |w, doc| $req.request(w, doc));
        res.map(CompilerQueryResponse::$method)
    }};
}

macro_rules! query_world {
    ($self:ident, $method:ident, $req:expr) => {{
        let res = $self.steal_world(move |w| $req.request(w));
        res.map(CompilerQueryResponse::$method)
    }};
}

impl TypstLanguageServer {
    pub fn query_source<T>(
        &self,
        path: ImmutPath,
        f: impl FnOnce(Source) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let snapshot = self.primary.memory_changes.get(&path);
        let snapshot = snapshot.ok_or_else(|| anyhow!("file missing {path:?}"))?;
        let source = snapshot.content.clone();
        f(source)
    }

    pub fn query(&mut self, query: CompilerQueryRequest) -> anyhow::Result<CompilerQueryResponse> {
        use CompilerQueryRequest::*;

        match query {
            InteractCodeContext(req) => query_source!(self, InteractCodeContext, req),
            SemanticTokensFull(req) => query_tokens_cache!(self, SemanticTokensFull, req),
            SemanticTokensDelta(req) => query_tokens_cache!(self, SemanticTokensDelta, req),
            FoldingRange(req) => query_source!(self, FoldingRange, req),
            SelectionRange(req) => query_source!(self, SelectionRange, req),
            DocumentSymbol(req) => query_source!(self, DocumentSymbol, req),
            ColorPresentation(req) => Ok(CompilerQueryResponse::ColorPresentation(req.request())),
            _ => {
                let client = &mut self.primary;
                if !self.pinning && !self.config.compile.has_default_entry_path {
                    // todo: race condition, we need atomic primary query
                    if let Some(path) = query.associated_path() {
                        client.do_change_entry(Some(path.into()))?;
                    }
                }
                Self::query_on(client.compiler(), query)
            }
        }
    }

    fn query_on(
        client: &CompileClientActor,
        query: CompilerQueryRequest,
    ) -> anyhow::Result<CompilerQueryResponse> {
        use CompilerQueryRequest::*;
        assert!(query.fold_feature() != FoldRequestFeature::ContextFreeUnique);

        match query {
            OnExport(OnExportRequest { kind, path }) => Ok(CompilerQueryResponse::OnExport(
                client.on_export(kind, path)?,
            )),
            OnSaveExport(OnSaveExportRequest { path }) => {
                client.on_save_export(path)?;
                Ok(CompilerQueryResponse::OnSaveExport(()))
            }
            Hover(req) => query_state!(client, Hover, req),
            GotoDefinition(req) => query_state!(client, GotoDefinition, req),
            GotoDeclaration(req) => query_world!(client, GotoDeclaration, req),
            References(req) => query_world!(client, References, req),
            InlayHint(req) => query_world!(client, InlayHint, req),
            DocumentHighlight(req) => query_world!(client, DocumentHighlight, req),
            DocumentColor(req) => query_world!(client, DocumentColor, req),
            CodeAction(req) => query_world!(client, CodeAction, req),
            CodeLens(req) => query_world!(client, CodeLens, req),
            Completion(req) => query_state!(client, Completion, req),
            SignatureHelp(req) => query_world!(client, SignatureHelp, req),
            Rename(req) => query_state!(client, Rename, req),
            PrepareRename(req) => query_state!(client, PrepareRename, req),
            Symbol(req) => query_world!(client, Symbol, req),
            DocumentMetrics(req) => query_state!(client, DocumentMetrics, req),
            ServerInfo(_) => {
                let res = client.collect_server_info()?;
                Ok(CompilerQueryResponse::ServerInfo(Some(res)))
            }
            _ => unreachable!(),
        }
    }
}

impl CompileServer {
    pub fn query(&self, query: CompilerQueryRequest) -> anyhow::Result<CompilerQueryResponse> {
        let client = self.compiler.as_ref().unwrap();
        TypstLanguageServer::query_on(client, query)
    }
}
