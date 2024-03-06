pub use tower_lsp::Client as LspHost;

use std::borrow::Cow;
use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use futures::FutureExt;
use log::{error, info, trace};
use once_cell::sync::OnceCell;
use serde_json::Value as JsonValue;
use tokio::sync::{Mutex, RwLock};
use tower_lsp::lsp_types::*;
use tower_lsp::{jsonrpc, LanguageServer};
use typst::model::Document;
use typst_ts_core::config::CompileOpts;

use crate::actor;
use crate::actor::typst::CompileCluster;
use crate::actor::typst::{
    CompilerQueryResponse, CompletionRequest, DocumentSymbolRequest, HoverRequest,
    OnSaveExportRequest, SelectionRangeRequest, SemanticTokensDeltaRequest,
    SemanticTokensFullRequest, SignatureHelpRequest, SymbolRequest,
};
use crate::config::{
    Config, ConstConfig, ExperimentalFormatterMode, ExportPdfMode, SemanticTokensMode,
};
use crate::ext::InitializeParamsExt;

use super::semantic_tokens::{
    get_semantic_tokens_options, get_semantic_tokens_registration,
    get_semantic_tokens_unregistration,
};

pub struct TypstServer {
    pub client: LspHost,
    pub document: Mutex<Arc<Document>>,
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
            document: Default::default(),
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
    ($self: expr, $query: ident, $req: expr) => {{
        let req = $req;
        $self
            .universe()
            .query(actor::typst::CompilerQueryRequest::$query(req.clone()))
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

#[async_trait]
impl LanguageServer for TypstServer {
    async fn initialize(&self, params: InitializeParams) -> jsonrpc::Result<InitializeResult> {
        // self.tracing_init();

        let cluster = {
            let root_paths = params.root_paths();
            let primary_root = root_paths.first().cloned().unwrap_or_default();
            actor::typst::create_cluster(
                self.client.clone(),
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

        self.const_config
            .set(ConstConfig::from(&params))
            .expect("const config should not yet be initialized");

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
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
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
            let _ = run_query!(self, OnSaveExport, OnSaveExportRequest { path });
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

    async fn hover(&self, params: HoverParams) -> jsonrpc::Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let path = uri.to_file_path().unwrap();
        let position = params.text_document_position_params.position;
        let position_encoding = self.const_config().position_encoding;

        run_query!(
            self,
            Hover,
            HoverRequest {
                path,
                position,
                position_encoding,
            }
        )
    }

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> jsonrpc::Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let path = uri.to_file_path().unwrap();
        let position = params.text_document_position.position;
        let explicit = params
            .context
            .map(|context| context.trigger_kind == CompletionTriggerKind::INVOKED)
            .unwrap_or(false);
        let position_encoding = self.const_config().position_encoding;

        run_query!(
            self,
            Completion,
            CompletionRequest {
                path,
                position,
                position_encoding,
                explicit,
            }
        )
    }

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> jsonrpc::Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let path = uri.to_file_path().unwrap();
        let position = params.text_document_position_params.position;
        let position_encoding = self.const_config().position_encoding;

        run_query!(
            self,
            SignatureHelp,
            SignatureHelpRequest {
                path,
                position,
                position_encoding,
            }
        )
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> jsonrpc::Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let path = uri.to_file_path().unwrap();
        let position_encoding = self.const_config().position_encoding;

        run_query!(
            self,
            DocumentSymbol,
            DocumentSymbolRequest {
                path,
                position_encoding
            }
        )
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> jsonrpc::Result<Option<Vec<SymbolInformation>>> {
        let pattern = (!params.query.is_empty()).then_some(params.query);
        let position_encoding = self.const_config().position_encoding;

        run_query!(
            self,
            Symbol,
            SymbolRequest {
                pattern,
                position_encoding
            }
        )
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> jsonrpc::Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri;
        let path = uri.to_file_path().unwrap();
        let positions = params.positions;
        let position_encoding = self.const_config().position_encoding;

        run_query!(
            self,
            SelectionRange,
            SelectionRangeRequest {
                path,
                positions,
                position_encoding
            }
        )
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> jsonrpc::Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let path = uri.to_file_path().unwrap();
        let position_encoding = self.const_config().position_encoding;

        run_query!(
            self,
            SemanticTokensFull,
            SemanticTokensFullRequest {
                path,
                position_encoding
            }
        )
    }

    async fn semantic_tokens_full_delta(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> jsonrpc::Result<Option<SemanticTokensFullDeltaResult>> {
        let uri = params.text_document.uri;
        let path = uri.to_file_path().unwrap();
        let previous_result_id = params.previous_result_id;
        let position_encoding = self.const_config().position_encoding;

        run_query!(
            self,
            SemanticTokensDelta,
            SemanticTokensDeltaRequest {
                path,
                previous_result_id,
                position_encoding
            }
        )
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

        let _ = run_query!(self, OnSaveExport, OnSaveExportRequest { path });

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
