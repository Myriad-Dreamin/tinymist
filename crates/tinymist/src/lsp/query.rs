//! tinymist's language server

use futures::future::MaybeDone;
use lsp_types::request::GotoDeclarationParams;
use lsp_types::*;
use serde::{Deserialize, Serialize};
use sync_ls::*;
use tinymist_query::{
    CompilerQueryRequest, CompilerQueryResponse, FoldRequestFeature, SyntaxRequest,
};
use tinymist_std::{ImmutPath, Result};

use crate::project::{EntryState, TaskInputs, DETACHED_ENTRY};
use crate::{as_path, as_path_, as_path_pos, FormatterMode, ServerState};

/// The future type for a lsp query.
pub type QueryFuture = Result<ResponseFuture<Result<CompilerQueryResponse>>>;

pub trait LspClientExt {
    fn schedule_query(&self, req_id: RequestId, query_fut: QueryFuture) -> ScheduledResult;
}

impl LspClientExt for LspClient {
    /// Schedules a query from the client.
    fn schedule_query(&self, req_id: RequestId, query_fut: QueryFuture) -> ScheduledResult {
        let fut = query_fut.map_err(|e| internal_error(e.to_string()))?;
        let fut: SchedulableResponse<CompilerQueryResponse> = Ok(match fut {
            MaybeDone::Done(res) => {
                MaybeDone::Done(res.map_err(|err| internal_error(err.to_string())))
            }
            MaybeDone::Future(fut) => MaybeDone::Future(Box::pin(async move {
                let res = fut.await;
                res.map_err(|err| internal_error(err.to_string()))
            })),
            MaybeDone::Gone => MaybeDone::Gone,
        });
        self.schedule(req_id, fut)
    }
}

macro_rules! run_query {
    ($req_id: ident, $self: ident.$query: ident ($($arg_key:ident),* $(,)?)) => {{
        use tinymist_query::*;
        let req = paste::paste! { [<$query Request>] { $($arg_key),* } };
        let query_fut = $self.query(CompilerQueryRequest::$query(req.clone()));
        $self.client.untyped().schedule_query($req_id, query_fut)
    }};
}
pub(crate) use run_query;

/// LSP Standard Language Features
impl ServerState {
    pub(crate) fn goto_definition(
        &mut self,
        req_id: RequestId,
        params: GotoDefinitionParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(req_id, self.GotoDefinition(path, position))
    }

    pub(crate) fn goto_declaration(
        &mut self,
        req_id: RequestId,
        params: GotoDeclarationParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(req_id, self.GotoDeclaration(path, position))
    }

    pub(crate) fn references(
        &mut self,
        req_id: RequestId,
        params: ReferenceParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position);
        run_query!(req_id, self.References(path, position))
    }

    pub(crate) fn hover(&mut self, req_id: RequestId, params: HoverParams) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        self.implicit_focus_entry(|| Some(path.as_path().into()), 'h');

        self.implicit_position = Some(position);
        run_query!(req_id, self.Hover(path, position))
    }

    pub(crate) fn folding_range(
        &mut self,
        req_id: RequestId,
        params: FoldingRangeParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        let line_folding_only = self.const_config().doc_line_folding_only;
        self.implicit_focus_entry(|| Some(path.as_path().into()), 'f');
        run_query!(req_id, self.FoldingRange(path, line_folding_only))
    }

    pub(crate) fn selection_range(
        &mut self,
        req_id: RequestId,
        params: SelectionRangeParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        let positions = params.positions;
        run_query!(req_id, self.SelectionRange(path, positions))
    }

    pub(crate) fn document_highlight(
        &mut self,
        req_id: RequestId,
        params: DocumentHighlightParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(req_id, self.DocumentHighlight(path, position))
    }

    pub(crate) fn document_symbol(
        &mut self,
        req_id: RequestId,
        params: DocumentSymbolParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        run_query!(req_id, self.DocumentSymbol(path))
    }

    pub(crate) fn semantic_tokens_full(
        &mut self,
        req_id: RequestId,
        params: SemanticTokensParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        self.implicit_focus_entry(|| Some(path.as_path().into()), 't');
        run_query!(req_id, self.SemanticTokensFull(path))
    }

    pub(crate) fn semantic_tokens_full_delta(
        &mut self,
        req_id: RequestId,
        params: SemanticTokensDeltaParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        let previous_result_id = params.previous_result_id;
        self.implicit_focus_entry(|| Some(path.as_path().into()), 't');
        run_query!(req_id, self.SemanticTokensDelta(path, previous_result_id))
    }

    pub(crate) fn formatting(
        &mut self,
        req_id: RequestId,
        params: DocumentFormattingParams,
    ) -> ScheduledResult {
        if matches!(self.config.formatter_mode, FormatterMode::Disable) {
            return Ok(None);
        }

        let path: ImmutPath = as_path(params.text_document).as_path().into();
        let source = self
            .query_source(path, |source: typst::syntax::Source| Ok(source))
            .map_err(|e| internal_error(format!("could not format document: {e}")))?;
        self.client.schedule(req_id, self.formatter.run(source))
    }

    pub(crate) fn inlay_hint(
        &mut self,
        req_id: RequestId,
        params: InlayHintParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        let range = params.range;
        run_query!(req_id, self.InlayHint(path, range))
    }

    pub(crate) fn document_color(
        &mut self,
        req_id: RequestId,
        params: DocumentColorParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        run_query!(req_id, self.DocumentColor(path))
    }

    pub(crate) fn document_link(
        &mut self,
        req_id: RequestId,
        params: DocumentLinkParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        run_query!(req_id, self.DocumentLink(path))
    }

    pub(crate) fn color_presentation(
        &mut self,
        req_id: RequestId,
        params: ColorPresentationParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        let color = params.color;
        let range = params.range;
        run_query!(req_id, self.ColorPresentation(path, color, range))
    }

    pub(crate) fn code_action(
        &mut self,
        req_id: RequestId,
        params: CodeActionParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        let range = params.range;
        run_query!(req_id, self.CodeAction(path, range))
    }

    pub(crate) fn code_lens(
        &mut self,
        req_id: RequestId,
        params: CodeLensParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        run_query!(req_id, self.CodeLens(path))
    }

    pub(crate) fn completion(
        &mut self,
        req_id: RequestId,
        params: CompletionParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position);
        let context = params.context.as_ref();
        let explicit =
            context.is_some_and(|context| context.trigger_kind == CompletionTriggerKind::INVOKED);
        let trigger_character = context
            .and_then(|c| c.trigger_character.as_ref())
            .and_then(|c| c.chars().next());

        self.implicit_position = Some(position);
        run_query!(
            req_id,
            self.Completion(path, position, explicit, trigger_character)
        )
    }

    pub(crate) fn signature_help(
        &mut self,
        req_id: RequestId,
        params: SignatureHelpParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position_params);

        self.implicit_position = Some(position);
        run_query!(req_id, self.SignatureHelp(path, position))
    }

    pub(crate) fn rename(&mut self, req_id: RequestId, params: RenameParams) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position);
        let new_name = params.new_name;
        run_query!(req_id, self.Rename(path, position, new_name))
    }

    pub(crate) fn prepare_rename(
        &mut self,
        req_id: RequestId,
        params: TextDocumentPositionParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params);
        run_query!(req_id, self.PrepareRename(path, position))
    }

    pub(crate) fn symbol(
        &mut self,
        req_id: RequestId,
        params: WorkspaceSymbolParams,
    ) -> ScheduledResult {
        let pattern = (!params.query.is_empty()).then_some(params.query);
        run_query!(req_id, self.Symbol(pattern))
    }

    pub(crate) fn on_enter(&mut self, req_id: RequestId, params: OnEnterParams) -> ScheduledResult {
        let path = as_path(params.text_document);
        let range = params.range;
        run_query!(req_id, self.OnEnter(path, range))
    }

    pub(crate) fn will_rename_files(
        &mut self,
        req_id: RequestId,
        params: RenameFilesParams,
    ) -> ScheduledResult {
        log::info!("will rename files {params:?}");
        let paths = params
            .files
            .iter()
            .map(|f| {
                Some((
                    as_path_(Url::parse(&f.old_uri).ok()?),
                    as_path_(Url::parse(&f.new_uri).ok()?),
                ))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| invalid_params("invalid urls"))?;

        run_query!(req_id, self.WillRenameFiles(paths))
    }
}

macro_rules! query_source {
    ($self:ident, $method:ident, $req:expr) => {{
        let path: ImmutPath = $req.path.clone().into();

        $self.query_source(path, |source| {
            let enc = $self.const_config().position_encoding;
            let res = $req.request(&source, enc);
            Ok(CompilerQueryResponse::$method(res))
        })
    }};
}

impl ServerState {
    /// Perform a language query.
    pub fn query(&mut self, query: CompilerQueryRequest) -> QueryFuture {
        use CompilerQueryRequest::*;

        just_ok(match query {
            FoldingRange(req) => query_source!(self, FoldingRange, req)?,
            SelectionRange(req) => query_source!(self, SelectionRange, req)?,
            DocumentSymbol(req) => query_source!(self, DocumentSymbol, req)?,
            OnEnter(req) => query_source!(self, OnEnter, req)?,
            ColorPresentation(req) => CompilerQueryResponse::ColorPresentation(req.request()),
            OnExport(req) => return self.on_export(req),
            ServerInfo(_) => return self.collect_server_info(),
            // todo: query on dedicate projects
            _ => return self.query_on(query),
        })
    }

    fn query_on(&mut self, query: CompilerQueryRequest) -> QueryFuture {
        use CompilerQueryRequest::*;
        type R = CompilerQueryResponse;
        assert!(query.fold_feature() != FoldRequestFeature::ContextFreeUnique);

        let (mut snap, stat) = self.query_snapshot_with_stat(&query)?;
        // todo: whether it is safe to inherit success_doc with changed entry
        if !self.is_pinning() {
            let input = query
                .associated_path()
                .map(|path| self.resolve_task(path.into()))
                .or_else(|| {
                    let root = self.entry_resolver().root(None)?;
                    Some(TaskInputs {
                        entry: Some(EntryState::new_rooted_by_id(root, *DETACHED_ENTRY)),
                        ..TaskInputs::default()
                    })
                });

            if let Some(input) = input {
                snap = snap.task(input);
            }
        }

        just_future(async move {
            stat.snap();

            if matches!(query, Completion(..)) {
                // Prefetch the package index for completion.
                if snap.world.registry.cached_index().is_none() {
                    let registry = snap.world.registry.clone();
                    tokio::spawn(async move {
                        let _ = registry.download_index();
                    });
                }
            }

            match query {
                SemanticTokensFull(req) => snap.run_semantic(req, R::SemanticTokensFull),
                SemanticTokensDelta(req) => snap.run_semantic(req, R::SemanticTokensDelta),
                InteractCodeContext(req) => snap.run_semantic(req, R::InteractCodeContext),
                Hover(req) => snap.run_stateful(req, R::Hover),
                GotoDefinition(req) => snap.run_stateful(req, R::GotoDefinition),
                GotoDeclaration(req) => snap.run_semantic(req, R::GotoDeclaration),
                References(req) => snap.run_stateful(req, R::References),
                InlayHint(req) => snap.run_semantic(req, R::InlayHint),
                DocumentHighlight(req) => snap.run_semantic(req, R::DocumentHighlight),
                DocumentColor(req) => snap.run_semantic(req, R::DocumentColor),
                DocumentLink(req) => snap.run_semantic(req, R::DocumentLink),
                CodeAction(req) => snap.run_semantic(req, R::CodeAction),
                CodeLens(req) => snap.run_semantic(req, R::CodeLens),
                Completion(req) => snap.run_stateful(req, R::Completion),
                SignatureHelp(req) => snap.run_semantic(req, R::SignatureHelp),
                Rename(req) => snap.run_stateful(req, R::Rename),
                WillRenameFiles(req) => snap.run_stateful(req, R::WillRenameFiles),
                PrepareRename(req) => snap.run_stateful(req, R::PrepareRename),
                Symbol(req) => snap.run_semantic(req, R::Symbol),
                WorkspaceLabel(req) => snap.run_semantic(req, R::WorkspaceLabel),
                DocumentMetrics(req) => snap.run_stateful(req, R::DocumentMetrics),
                _ => unreachable!(),
            }
        })
    }
}

/// A parameter for the `experimental/onEnter` command.
///
/// @since 3.17.0
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnEnterParams {
    /// The text document.
    pub text_document: TextDocumentIdentifier,

    /// The visible document range for which `onEnter` edits should be computed.
    pub range: Range,
}

pub struct OnEnter;
impl lsp_types::request::Request for OnEnter {
    type Params = OnEnterParams;
    type Result = Option<Vec<TextEdit>>;
    const METHOD: &'static str = "experimental/onEnter";
}
