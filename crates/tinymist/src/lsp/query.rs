//! tinymist's language server

use lsp_types::request::GotoDeclarationParams;
use lsp_types::*;
use serde::{Deserialize, Serialize};
use sync_ls::*;
use tinymist_query::{
    CompilerQueryRequest, CompilerQueryResponse, FoldRequestFeature, SyntaxRequest,
};
use tinymist_std::ImmutPath;

use crate::project::{EntryState, TaskInputs, DETACHED_ENTRY};
use crate::{as_path, as_path_, as_path_pos, FormatterMode, ServerState};

/// The future type for a lsp query.
pub type QueryFuture = SchedulableResponse<CompilerQueryResponse>;

macro_rules! run_query {
    ($self: ident.$query: ident ($($arg_key:ident),* $(,)?)) => {{
        use tinymist_query::*;
        let req = paste::paste! { [<$query Request>] { $($arg_key),* } };
        erased_response($self.query(CompilerQueryRequest::$query(req.clone())))
    }};
}
pub(crate) use run_query;

/// LSP Standard Language Features
impl ServerState {
    pub(crate) fn goto_definition(&mut self, params: GotoDefinitionParams) -> ScheduleResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(self.GotoDefinition(path, position))
    }

    pub(crate) fn goto_declaration(&mut self, params: GotoDeclarationParams) -> ScheduleResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(self.GotoDeclaration(path, position))
    }

    pub(crate) fn references(&mut self, params: ReferenceParams) -> ScheduleResult {
        let (path, position) = as_path_pos(params.text_document_position);
        run_query!(self.References(path, position))
    }

    pub(crate) fn hover(&mut self, params: HoverParams) -> ScheduleResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        self.implicit_focus_entry(|| Some(path.as_path().into()), 'h');

        self.implicit_position = Some(position);
        run_query!(self.Hover(path, position))
    }

    pub(crate) fn folding_range(&mut self, params: FoldingRangeParams) -> ScheduleResult {
        let path = as_path(params.text_document);
        let line_folding_only = self.const_config().doc_line_folding_only;
        self.implicit_focus_entry(|| Some(path.as_path().into()), 'f');
        run_query!(self.FoldingRange(path, line_folding_only))
    }

    pub(crate) fn selection_range(&mut self, params: SelectionRangeParams) -> ScheduleResult {
        let path = as_path(params.text_document);
        let positions = params.positions;
        run_query!(self.SelectionRange(path, positions))
    }

    pub(crate) fn document_highlight(&mut self, params: DocumentHighlightParams) -> ScheduleResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(self.DocumentHighlight(path, position))
    }

    pub(crate) fn document_symbol(&mut self, params: DocumentSymbolParams) -> ScheduleResult {
        let path = as_path(params.text_document);
        run_query!(self.DocumentSymbol(path))
    }

    pub(crate) fn semantic_tokens_full(&mut self, params: SemanticTokensParams) -> ScheduleResult {
        let path = as_path(params.text_document);
        self.implicit_focus_entry(|| Some(path.as_path().into()), 't');
        run_query!(self.SemanticTokensFull(path))
    }

    pub(crate) fn semantic_tokens_full_delta(
        &mut self,

        params: SemanticTokensDeltaParams,
    ) -> ScheduleResult {
        let path = as_path(params.text_document);
        let previous_result_id = params.previous_result_id;
        self.implicit_focus_entry(|| Some(path.as_path().into()), 't');
        run_query!(self.SemanticTokensDelta(path, previous_result_id))
    }

    pub(crate) fn formatting(&mut self, params: DocumentFormattingParams) -> ScheduleResult {
        if matches!(self.config.formatter_mode, FormatterMode::Disable) {
            return just_ok(serde_json::Value::Null);
        }

        let path: ImmutPath = as_path(params.text_document).as_path().into();
        let source = self.query_source(path, |source: typst::syntax::Source| Ok(source))?;
        erased_response(self.formatter.run(source))
    }

    pub(crate) fn range_formatting(
        &mut self,
        params: DocumentRangeFormattingParams,
    ) -> ScheduleResult {
        if matches!(self.config.formatter_mode, FormatterMode::Disable) {
            return just_ok(serde_json::Value::Null);
        }

        let path: ImmutPath = as_path(params.text_document).as_path().into();
        let source = self.query_source(path, Ok)?;
        erased_response(self.formatter.run_on_range(source, params.range))
    }

    pub(crate) fn inlay_hint(&mut self, params: InlayHintParams) -> ScheduleResult {
        let path = as_path(params.text_document);
        let range = params.range;
        run_query!(self.InlayHint(path, range))
    }

    pub(crate) fn document_color(&mut self, params: DocumentColorParams) -> ScheduleResult {
        let path = as_path(params.text_document);
        run_query!(self.DocumentColor(path))
    }

    pub(crate) fn document_link(&mut self, params: DocumentLinkParams) -> ScheduleResult {
        let path = as_path(params.text_document);
        run_query!(self.DocumentLink(path))
    }

    pub(crate) fn color_presentation(&mut self, params: ColorPresentationParams) -> ScheduleResult {
        let path = as_path(params.text_document);
        let color = params.color;
        let range = params.range;
        run_query!(self.ColorPresentation(path, color, range))
    }

    pub(crate) fn code_action(&mut self, params: CodeActionParams) -> ScheduleResult {
        let path = as_path(params.text_document);
        let range = params.range;
        let context = params.context;
        run_query!(self.CodeAction(path, range, context))
    }

    pub(crate) fn code_lens(&mut self, params: CodeLensParams) -> ScheduleResult {
        let path = as_path(params.text_document);
        run_query!(self.CodeLens(path))
    }

    pub(crate) fn completion(&mut self, params: CompletionParams) -> ScheduleResult {
        let (path, position) = as_path_pos(params.text_document_position);
        let context = params.context.as_ref();
        let explicit =
            context.is_some_and(|context| context.trigger_kind == CompletionTriggerKind::INVOKED);
        let trigger_character = context
            .and_then(|c| c.trigger_character.as_ref())
            .and_then(|c| c.chars().next());

        self.implicit_position = Some(position);
        run_query!(self.Completion(path, position, explicit, trigger_character))
    }

    pub(crate) fn signature_help(&mut self, params: SignatureHelpParams) -> ScheduleResult {
        let (path, position) = as_path_pos(params.text_document_position_params);

        self.implicit_position = Some(position);
        run_query!(self.SignatureHelp(path, position))
    }

    pub(crate) fn rename(&mut self, params: RenameParams) -> ScheduleResult {
        let (path, position) = as_path_pos(params.text_document_position);
        let new_name = params.new_name;
        run_query!(self.Rename(path, position, new_name))
    }

    pub(crate) fn prepare_rename(&mut self, params: TextDocumentPositionParams) -> ScheduleResult {
        let (path, position) = as_path_pos(params);
        run_query!(self.PrepareRename(path, position))
    }

    pub(crate) fn symbol(&mut self, params: WorkspaceSymbolParams) -> ScheduleResult {
        let pattern = (!params.query.is_empty()).then_some(params.query);
        run_query!(self.Symbol(pattern))
    }

    pub(crate) fn on_enter(&mut self, params: OnEnterParams) -> ScheduleResult {
        let path = as_path(params.text_document);
        let range = params.range;
        let handle_list = self.config.on_enter.handle_list;
        run_query!(self.OnEnter(path, range, handle_list))
    }

    pub(crate) fn will_rename_files(&mut self, params: RenameFilesParams) -> ScheduleResult {
        log::info!("will rename files {params:?}");
        let paths = params
            .files
            .iter()
            .map(|f| {
                Some((
                    as_path_(&Url::parse(&f.old_uri).ok()?),
                    as_path_(&Url::parse(&f.new_uri).ok()?),
                ))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| invalid_params("invalid urls"))?;

        run_query!(self.WillRenameFiles(paths))
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
            #[cfg(feature = "export")]
            OnExport(req) => return self.on_export(req),
            #[cfg(not(feature = "export"))]
            OnExport(_req) => return Err(internal_error("export feature is not enabled")),
            ServerInfo(_) => return self.collect_server_info(),
            // todo: query on dedicate projects
            _ => return self.query_on(query),
        })
    }

    fn query_on(&mut self, query: CompilerQueryRequest) -> QueryFuture {
        use CompilerQueryRequest::*;
        type R = CompilerQueryResponse;
        assert!(query.fold_feature() != FoldRequestFeature::ContextFreeUnique);

        let (mut snap, stat) = self
            .query_snapshot_with_stat(&query)
            .map_err(internal_error)?;
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

            // todo: preload in web
            #[cfg(feature = "system")]
            if matches!(query, Completion(..)) {
                // Prefetch the package index for completion.
                if snap.registry().cached_index().is_none() {
                    let registry = snap.registry().clone();
                    tokio::spawn(async move {
                        let _ = registry.download_index();
                    });
                }
            }

            let res = match query {
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
            };

            res.map_err(internal_error)
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
