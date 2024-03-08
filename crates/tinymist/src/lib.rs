// pub mod formatting;
pub mod actor;

pub use tower_lsp::Client as LspHost;

use core::fmt;
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context};
use futures::{future::BoxFuture, FutureExt};
use itertools::Itertools;
use log::{error, info, trace};
use once_cell::sync::OnceCell;
use paste::paste;
use serde::Deserialize;
use serde_json::{Map, Value as JsonValue};
use tinymist_query::{
    get_semantic_tokens_options, get_semantic_tokens_registration,
    get_semantic_tokens_unregistration, PositionEncoding,
};
use tokio::sync::RwLock;
use tower_lsp::{jsonrpc, lsp_types::*, LanguageServer};
use typst_ts_core::config::CompileOpts;

use crate::actor::typst::CompileCluster;

pub struct TypstServer {
    pub client: LspHost,
    // typst_thread: TypstThread,
    pub universe: OnceCell<CompileCluster>,
    pub config: Arc<RwLock<Config>>,
    pub const_config: OnceCell<ConstConfig>,
}

impl TypstServer {
    pub fn new(client: LspHost) -> Self {
        Self {
            // typst_thread: Default::default(),
            universe: Default::default(),
            config: Default::default(),
            const_config: Default::default(),
            client,
        }
    }

    pub fn const_config(&self) -> &ConstConfig {
        self.const_config
            .get()
            .expect("const config should be initialized")
    }

    pub fn universe(&self) -> &CompileCluster {
        self.universe.get().expect("universe should be initialized")
    }
}

macro_rules! run_query {
    ($self: ident.$query: ident ($($arg_key:ident),+ $(,)?)) => {{
        use tinymist_query::*;
        let req = paste! { [<$query Request>] { $($arg_key),+ } };
        $self
            .universe()
            .query(CompilerQueryRequest::$query(req.clone()))
            .await
            .map_err(|err| {
                error!("error getting $query: {err} with request {req:?}");
                jsonrpc::Error::internal_error()
            })
            .map(|resp| {
                let CompilerQueryResponse::$query(resp) = resp else {
                    unreachable!()
                };
                resp
            })
    }};
}

fn as_path(inp: TextDocumentIdentifier) -> PathBuf {
    inp.uri.to_file_path().unwrap()
}

fn as_path_pos(inp: TextDocumentPositionParams) -> (PathBuf, Position) {
    (as_path(inp.text_document), inp.position)
}

#[async_trait::async_trait]
impl LanguageServer for TypstServer {
    async fn initialize(&self, params: InitializeParams) -> jsonrpc::Result<InitializeResult> {
        // self.tracing_init();

        self.const_config
            .set(ConstConfig::from(&params))
            .expect("const config should not yet be initialized");

        let cluster = {
            let root_paths = params.root_paths();
            let primary_root = root_paths.first().cloned().unwrap_or_default();
            actor::typst::create_cluster(
                self.client.clone(),
                self.const_config.get().unwrap(),
                root_paths,
                CompileOpts {
                    root_dir: primary_root,
                    // todo: font paths
                    // font_paths: arguments.font_paths.clone(),
                    with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
                    ..CompileOpts::default()
                },
            )
        };

        let (cluster, cluster_bg) = cluster.split();

        self.universe
            .set(cluster)
            .map_err(|_| ())
            .expect("the cluster is already initialized");

        tokio::spawn(cluster_bg.run());

        if let Some(init) = &params.initialization_options {
            let mut config = self.config.write().await;
            config
                .update(init)
                .await
                .as_ref()
                .map_err(ToString::to_string)
                .map_err(jsonrpc::Error::invalid_params)?;
        }

        let config = self.config.read().await;

        let semantic_tokens_provider = match config.semantic_tokens {
            SemanticTokensMode::Enable
                if !params.supports_semantic_tokens_dynamic_registration() =>
            {
                Some(get_semantic_tokens_options().into())
            }
            _ => None,
        };

        let document_formatting_provider = match config.formatter {
            ExperimentalFormatterMode::On
                if !params.supports_document_formatting_dynamic_registration() =>
            {
                Some(OneOf::Left(true))
            }
            _ => None,
        };

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        String::from("#"),
                        String::from("."),
                        String::from("@"),
                    ]),
                    ..Default::default()
                }),
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                        ..Default::default()
                    },
                )),
                semantic_tokens_provider,
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: LspCommand::all_as_string(),
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    ..Default::default()
                }),
                document_formatting_provider,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let const_config = self.const_config();
        let mut config = self.config.write().await;

        if const_config.supports_semantic_tokens_dynamic_registration {
            trace!("setting up to dynamically register semantic token support");

            let client = self.client.clone();
            let register = move || {
                trace!("dynamically registering semantic tokens");
                let client = client.clone();
                async move {
                    let options = get_semantic_tokens_options();
                    client
                        .register_capability(vec![get_semantic_tokens_registration(options)])
                        .await
                        .context("could not register semantic tokens")
                }
            };

            let client = self.client.clone();
            let unregister = move || {
                trace!("unregistering semantic tokens");
                let client = client.clone();
                async move {
                    client
                        .unregister_capability(vec![get_semantic_tokens_unregistration()])
                        .await
                        .context("could not unregister semantic tokens")
                }
            };

            if config.semantic_tokens == SemanticTokensMode::Enable {
                if let Some(err) = register().await.err() {
                    error!("could not dynamically register semantic tokens: {err}");
                }
            }

            config.listen_semantic_tokens(Box::new(move |mode| match mode {
                SemanticTokensMode::Enable => register().boxed(),
                SemanticTokensMode::Disable => unregister().boxed(),
            }));
        }

        if const_config.supports_config_change_registration {
            trace!("setting up to request config change notifications");

            const CONFIG_REGISTRATION_ID: &str = "config";
            const CONFIG_METHOD_ID: &str = "workspace/didChangeConfiguration";

            let err = self
                .client
                .register_capability(vec![Registration {
                    id: CONFIG_REGISTRATION_ID.to_owned(),
                    method: CONFIG_METHOD_ID.to_owned(),
                    register_options: None,
                }])
                .await
                .err();
            if let Some(err) = err {
                error!("could not register to watch config changes: {err}");
            }
        }

        info!("server initialized");
    }

    async fn shutdown(&self) -> jsonrpc::Result<()> {
        Ok(())
    }

    // Document Synchronization

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let path = params.text_document.uri.to_file_path().unwrap();
        let text = params.text_document.text;

        let universe = self.universe();
        universe.create_source(path.clone(), text).await.unwrap();
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let path = params.text_document.uri.to_file_path().unwrap();

        let universe = self.universe();
        universe.remove_source(path.clone()).await.unwrap();
        // self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let path = params.text_document.uri.to_file_path().unwrap();
        let changes = params.content_changes;

        let universe = self.universe();
        universe
            .edit_source(path.clone(), changes, self.const_config().position_encoding)
            .await
            .unwrap();
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        let path = uri.to_file_path().unwrap();

        let config = self.config.read().await;

        if config.export_pdf == ExportPdfMode::OnSave {
            let _ = run_query!(self.OnSaveExport(path));
        }
    }

    // Language Features

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> jsonrpc::Result<Option<GotoDefinitionResponse>> {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(self.GotoDefinition(path, position))
    }

    async fn hover(&self, params: HoverParams) -> jsonrpc::Result<Option<Hover>> {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(self.Hover(path, position))
    }

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> jsonrpc::Result<Option<Vec<FoldingRange>>> {
        let path = as_path(params.text_document);
        let line_folding_only = self.const_config().line_folding_only;
        run_query!(self.FoldingRange(path, line_folding_only))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> jsonrpc::Result<Option<Vec<SelectionRange>>> {
        let path = as_path(params.text_document);
        let positions = params.positions;
        run_query!(self.SelectionRange(path, positions))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> jsonrpc::Result<Option<DocumentSymbolResponse>> {
        let path = as_path(params.text_document);
        run_query!(self.DocumentSymbol(path))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> jsonrpc::Result<Option<SemanticTokensResult>> {
        let path = as_path(params.text_document);
        run_query!(self.SemanticTokensFull(path))
    }

    async fn semantic_tokens_full_delta(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> jsonrpc::Result<Option<SemanticTokensFullDeltaResult>> {
        let path = as_path(params.text_document);
        let previous_result_id = params.previous_result_id;
        run_query!(self.SemanticTokensDelta(path, previous_result_id))
    }

    async fn inlay_hint(
        &self,
        _params: InlayHintParams,
    ) -> jsonrpc::Result<Option<Vec<InlayHint>>> {
        Ok(None)
    }

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> jsonrpc::Result<Option<CompletionResponse>> {
        let (path, position) = as_path_pos(params.text_document_position);
        let explicit = params
            .context
            .map(|context| context.trigger_kind == CompletionTriggerKind::INVOKED)
            .unwrap_or(false);

        run_query!(self.Completion(path, position, explicit))
    }

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> jsonrpc::Result<Option<SignatureHelp>> {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(self.SignatureHelp(path, position))
    }

    async fn rename(&self, _params: RenameParams) -> jsonrpc::Result<Option<WorkspaceEdit>> {
        Ok(None)
    }

    async fn prepare_rename(
        &self,
        _params: TextDocumentPositionParams,
    ) -> jsonrpc::Result<Option<PrepareRenameResponse>> {
        Ok(None)
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> jsonrpc::Result<Option<Vec<SymbolInformation>>> {
        let pattern = (!params.query.is_empty()).then_some(params.query);
        run_query!(self.Symbol(pattern))
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        // For some clients, we don't get the actual changed configuration and need to
        // poll for it https://github.com/microsoft/language-server-protocol/issues/676
        let values = match params.settings {
            JsonValue::Object(settings) => Ok(settings),
            _ => self
                .client
                .configuration(Config::get_items())
                .await
                .map(Config::values_to_map),
        };

        let result = match values {
            Ok(values) => {
                let mut config = self.config.write().await;
                config.update_by_map(&values).await
            }
            Err(err) => Err(err.into()),
        };

        match result {
            Ok(()) => {
                info!("new settings applied");
            }
            Err(err) => {
                error!("error applying new settings: {err}");
            }
        }
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> jsonrpc::Result<Option<JsonValue>> {
        let ExecuteCommandParams {
            command,
            arguments,
            work_done_progress_params: _,
        } = params;
        match LspCommand::parse(&command) {
            Some(LspCommand::ExportPdf) => {
                self.command_export_pdf(arguments).await?;
            }
            Some(LspCommand::ClearCache) => {
                self.command_clear_cache(arguments).await?;
            }
            None => {
                error!("asked to execute unknown command");
                return Err(jsonrpc::Error::method_not_found());
            }
        };
        Ok(None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspCommand {
    ExportPdf,
    ClearCache,
}

impl From<LspCommand> for String {
    fn from(command: LspCommand) -> Self {
        match command {
            LspCommand::ExportPdf => "tinymist.doPdfExport".to_string(),
            LspCommand::ClearCache => "tinymist.doClearCache".to_string(),
        }
    }
}

impl LspCommand {
    pub fn parse(command: &str) -> Option<Self> {
        match command {
            "tinymist.doPdfExport" => Some(Self::ExportPdf),
            "tinymist.doClearCache" => Some(Self::ClearCache),
            _ => None,
        }
    }

    pub fn all_as_string() -> Vec<String> {
        vec![Self::ExportPdf.into(), Self::ClearCache.into()]
    }
}

/// Here are implemented the handlers for each command.
impl TypstServer {
    /// Export the current document as a PDF file. The client is responsible for
    /// passing the correct file URI.
    pub async fn command_export_pdf(&self, arguments: Vec<JsonValue>) -> jsonrpc::Result<()> {
        if arguments.is_empty() {
            return Err(jsonrpc::Error::invalid_params("Missing file URI argument"));
        }
        let Some(file_uri) = arguments.first().and_then(|v| v.as_str()) else {
            return Err(jsonrpc::Error::invalid_params(
                "Missing file URI as first argument",
            ));
        };
        let file_uri = Url::parse(file_uri)
            .map_err(|_| jsonrpc::Error::invalid_params("Parameter is not a valid URI"))?;
        let path = file_uri
            .to_file_path()
            .map_err(|_| jsonrpc::Error::invalid_params("URI is not a file URI"))?;

        let _ = run_query!(self.OnSaveExport(path));

        Ok(())
    }

    /// Clear all cached resources.
    pub async fn command_clear_cache(&self, _arguments: Vec<JsonValue>) -> jsonrpc::Result<()> {
        // self.workspace().write().await.clear().map_err(|err| {
        //     error!("could not clear cache: {err}");
        //     jsonrpc::Error::internal_error()
        // })?;

        // self.typst(|_| comemo::evict(0)).await;

        // Ok(())

        todo!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExperimentalFormatterMode {
    #[default]
    Off,
    On,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportPdfMode {
    Never,
    #[default]
    OnSave,
    OnType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticTokensMode {
    Disable,
    #[default]
    Enable,
}

pub type Listener<T> = Box<dyn FnMut(&T) -> BoxFuture<anyhow::Result<()>> + Send + Sync>;

const CONFIG_ITEMS: &[&str] = &[
    "exportPdf",
    "rootPath",
    "semanticTokens",
    "experimentalFormatterMode",
];

#[derive(Default)]
pub struct Config {
    pub export_pdf: ExportPdfMode,
    pub root_path: Option<PathBuf>,
    pub semantic_tokens: SemanticTokensMode,
    pub formatter: ExperimentalFormatterMode,
    semantic_tokens_listeners: Vec<Listener<SemanticTokensMode>>,
    formatter_listeners: Vec<Listener<ExperimentalFormatterMode>>,
}

impl Config {
    pub fn get_items() -> Vec<ConfigurationItem> {
        let sections = CONFIG_ITEMS
            .iter()
            .flat_map(|item| [format!("tinymist.{item}"), item.to_string()]);

        sections
            .map(|section| ConfigurationItem {
                section: Some(section),
                ..Default::default()
            })
            .collect()
    }

    pub fn values_to_map(values: Vec<JsonValue>) -> Map<String, JsonValue> {
        let unpaired_values = values
            .into_iter()
            .tuples()
            .map(|(a, b)| if !a.is_null() { a } else { b });

        CONFIG_ITEMS
            .iter()
            .map(|item| item.to_string())
            .zip(unpaired_values)
            .collect()
    }

    pub fn listen_semantic_tokens(&mut self, listener: Listener<SemanticTokensMode>) {
        self.semantic_tokens_listeners.push(listener);
    }

    // pub fn listen_formatting(&mut self, listener:
    // Listener<ExperimentalFormatterMode>) {     self.formatter_listeners.
    // push(listener); }

    pub async fn update(&mut self, update: &JsonValue) -> anyhow::Result<()> {
        if let JsonValue::Object(update) = update {
            self.update_by_map(update).await
        } else {
            bail!("got invalid configuration object {update}")
        }
    }

    pub async fn update_by_map(&mut self, update: &Map<String, JsonValue>) -> anyhow::Result<()> {
        let export_pdf = update
            .get("exportPdf")
            .map(ExportPdfMode::deserialize)
            .and_then(Result::ok);
        if let Some(export_pdf) = export_pdf {
            self.export_pdf = export_pdf;
        }

        let root_path = update.get("rootPath");
        if let Some(root_path) = root_path {
            if root_path.is_null() {
                self.root_path = None;
            }
            if let Some(root_path) = root_path.as_str().map(PathBuf::from) {
                self.root_path = Some(root_path);
            }
        }

        let semantic_tokens = update
            .get("semanticTokens")
            .map(SemanticTokensMode::deserialize)
            .and_then(Result::ok);
        if let Some(semantic_tokens) = semantic_tokens {
            for listener in &mut self.semantic_tokens_listeners {
                listener(&semantic_tokens).await?;
            }
            self.semantic_tokens = semantic_tokens;
        }

        let formatter = update
            .get("experimentalFormatterMode")
            .map(ExperimentalFormatterMode::deserialize)
            .and_then(Result::ok);
        if let Some(formatter) = formatter {
            for listener in &mut self.formatter_listeners {
                listener(&formatter).await?;
            }
            self.formatter = formatter;
        }

        Ok(())
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Config")
            .field("export_pdf", &self.export_pdf)
            .field("formatter", &self.formatter)
            .field("semantic_tokens", &self.semantic_tokens)
            .field(
                "semantic_tokens_listeners",
                &format_args!("Vec[len = {}]", self.semantic_tokens_listeners.len()),
            )
            .field(
                "formatter_listeners",
                &format_args!("Vec[len = {}]", self.formatter_listeners.len()),
            )
            .finish()
    }
}

/// Configuration set at initialization that won't change within a single
/// session
#[derive(Debug)]
pub struct ConstConfig {
    pub position_encoding: PositionEncoding,
    pub supports_semantic_tokens_dynamic_registration: bool,
    pub supports_document_formatting_dynamic_registration: bool,
    pub supports_config_change_registration: bool,
    pub line_folding_only: bool,
}

impl ConstConfig {
    fn choose_encoding(params: &InitializeParams) -> PositionEncoding {
        let encodings = params.position_encodings();
        if encodings.contains(&PositionEncodingKind::UTF8) {
            PositionEncoding::Utf8
        } else {
            PositionEncoding::Utf16
        }
    }
}

impl From<&InitializeParams> for ConstConfig {
    fn from(params: &InitializeParams) -> Self {
        Self {
            position_encoding: Self::choose_encoding(params),
            supports_semantic_tokens_dynamic_registration: params
                .supports_semantic_tokens_dynamic_registration(),
            supports_document_formatting_dynamic_registration: params
                .supports_document_formatting_dynamic_registration(),
            supports_config_change_registration: params.supports_config_change_registration(),
            line_folding_only: params.line_folding_only(),
        }
    }
}

pub trait InitializeParamsExt {
    fn position_encodings(&self) -> &[PositionEncodingKind];
    fn supports_config_change_registration(&self) -> bool;
    fn semantic_tokens_capabilities(&self) -> Option<&SemanticTokensClientCapabilities>;
    fn document_formatting_capabilities(&self) -> Option<&DocumentFormattingClientCapabilities>;
    fn supports_semantic_tokens_dynamic_registration(&self) -> bool;
    fn supports_document_formatting_dynamic_registration(&self) -> bool;
    fn line_folding_only(&self) -> bool;
    fn root_paths(&self) -> Vec<PathBuf>;
}

static DEFAULT_ENCODING: [PositionEncodingKind; 1] = [PositionEncodingKind::UTF16];

impl InitializeParamsExt for InitializeParams {
    fn position_encodings(&self) -> &[PositionEncodingKind] {
        self.capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_ref())
            .map(|encodings| encodings.as_slice())
            .unwrap_or(&DEFAULT_ENCODING)
    }

    fn supports_config_change_registration(&self) -> bool {
        self.capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.configuration)
            .unwrap_or(false)
    }

    fn line_folding_only(&self) -> bool {
        self.capabilities
            .text_document
            .as_ref()
            .and_then(|workspace| workspace.folding_range.as_ref())
            .and_then(|folding| folding.line_folding_only)
            .unwrap_or(false)
    }

    fn semantic_tokens_capabilities(&self) -> Option<&SemanticTokensClientCapabilities> {
        self.capabilities
            .text_document
            .as_ref()?
            .semantic_tokens
            .as_ref()
    }

    fn document_formatting_capabilities(&self) -> Option<&DocumentFormattingClientCapabilities> {
        self.capabilities
            .text_document
            .as_ref()?
            .formatting
            .as_ref()
    }

    fn supports_semantic_tokens_dynamic_registration(&self) -> bool {
        self.semantic_tokens_capabilities()
            .and_then(|semantic_tokens| semantic_tokens.dynamic_registration)
            .unwrap_or(false)
    }

    fn supports_document_formatting_dynamic_registration(&self) -> bool {
        self.document_formatting_capabilities()
            .and_then(|document_format| document_format.dynamic_registration)
            .unwrap_or(false)
    }

    #[allow(deprecated)] // `self.root_path` is marked as deprecated
    fn root_paths(&self) -> Vec<PathBuf> {
        match self.workspace_folders.as_ref() {
            Some(roots) => roots
                .iter()
                .map(|root| &root.uri)
                .map(Url::to_file_path)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            None => self
                .root_uri
                .as_ref()
                .map(|uri| uri.to_file_path().unwrap())
                .or_else(|| self.root_path.clone().map(PathBuf::from))
                .into_iter()
                .collect(),
        }
    }
}
