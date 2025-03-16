use lsp_types::request::WorkspaceConfiguration;
use lsp_types::*;
use once_cell::sync::OnceCell;
use reflexo::ImmutPath;
use request::{RegisterCapability, UnregisterCapability};
use serde_json::{Map, Value as JsonValue};
use sync_ls::*;
use tinymist_std::error::{prelude::*, IgnoreLogging};

pub mod init;
pub(crate) mod query;

use crate::task::FormatterConfig;
use crate::*;

/// Trait implemented by language server backends.
///
/// This interface allows servers adhering to the [Language Server Protocol] to
/// be implemented in a safe and easily testable way without exposing the
/// low-level implementation details.
///
/// [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
impl ServerState {
    /// The [`initialized`] notification is sent from the client to the server
    /// after the client received the result of the initialize request but
    /// before the client sends anything else.
    ///
    /// [`initialized`]: https://microsoft.github.io/language-server-protocol/specification#initialized
    ///
    /// The server can use the `initialized` notification, for example, to
    /// dynamically register capabilities with the client.
    pub(crate) fn initialized(&mut self, _params: InitializedParams) -> LspResult<()> {
        if self.const_config().tokens_dynamic_registration
            && self.config.semantic_tokens == SemanticTokensMode::Enable
        {
            self.enable_sema_token_caps(true)
                .log_error("could not register semantic tokens for initialization");
        }

        if self.const_config().doc_fmt_dynamic_registration
            && self.config.formatter_mode != FormatterMode::Disable
        {
            self.enable_formatter_caps(true)
                .log_error("could not register formatter for initialization");
        }

        if self.const_config().cfg_change_registration {
            log::trace!("setting up to request config change notifications");

            const CONFIG_REGISTRATION_ID: &str = "config";
            const CONFIG_METHOD_ID: &str = "workspace/didChangeConfiguration";

            self.register_capability(vec![Registration {
                id: CONFIG_REGISTRATION_ID.to_owned(),
                method: CONFIG_METHOD_ID.to_owned(),
                register_options: None,
            }])
            .log_error("could not register to watch config changes");
        }

        log::info!("server initialized");
        Ok(())
    }

    /// The [`shutdown`] request asks the server to gracefully shut down, but to
    /// not exit.
    ///
    /// [`shutdown`]: https://microsoft.github.io/language-server-protocol/specification#shutdown
    ///
    /// This request is often later followed by an [`exit`] notification, which
    /// will cause the server to exit immediately.
    ///
    /// [`exit`]: https://microsoft.github.io/language-server-protocol/specification#exit
    ///
    /// This method is guaranteed to only execute once. If the client sends this
    /// request to the server again, the server will respond with JSON-RPC
    /// error code `-32600` (invalid request).
    pub(crate) fn shutdown(&mut self, _params: ()) -> SchedulableResponse<()> {
        just_ok(())
    }
}

/// LSP Document Synchronization
impl ServerState {
    pub(crate) fn did_open(&mut self, params: DidOpenTextDocumentParams) -> LspResult<()> {
        log::info!("did open {}", params.text_document.uri);
        let path: ImmutPath = as_path_(params.text_document.uri).as_path().into();
        let text = params.text_document.text;

        self.create_source(path.clone(), text)
            .map_err(invalid_params)?;

        // Focus after opening
        self.implicit_focus_entry(|| Some(path), 'o');
        Ok(())
    }

    pub(crate) fn did_close(&mut self, params: DidCloseTextDocumentParams) -> LspResult<()> {
        let path = as_path_(params.text_document.uri).as_path().into();

        self.remove_source(path).map_err(invalid_params)?;
        Ok(())
    }

    pub(crate) fn did_change(&mut self, params: DidChangeTextDocumentParams) -> LspResult<()> {
        let path = as_path_(params.text_document.uri).as_path().into();
        let changes = params.content_changes;

        self.edit_source(path, changes, self.const_config().position_encoding)
            .map_err(invalid_params)?;
        Ok(())
    }

    pub(crate) fn did_save(&mut self, _params: DidSaveTextDocumentParams) -> LspResult<()> {
        Ok(())
    }
}

/// LSP Configuration Synchronization
impl ServerState {
    pub(crate) fn on_changed_configuration(
        &mut self,
        values: Map<String, JsonValue>,
    ) -> LspResult<()> {
        let old_config = self.config.clone();
        match self.config.update_by_map(&values) {
            Ok(()) => {}
            Err(err) => {
                self.config = old_config;
                log::error!("error applying new settings: {err}");
                return Err(invalid_params(format!(
                    "error applying new settings: {err}"
                )));
            }
        }

        let new_export_config = self.config.export();
        if old_config.export() != new_export_config {
            self.change_export_config(new_export_config);
        }

        if old_config.compile.primary_opts() != self.config.compile.primary_opts() {
            self.config.compile.fonts = OnceCell::new(); // todo: don't reload fonts if not changed
            self.reload_projects()
                .log_error("could not restart primary");
        }

        if old_config.semantic_tokens != self.config.semantic_tokens {
            self.enable_sema_token_caps(self.config.semantic_tokens == SemanticTokensMode::Enable)
                .log_error("could not change semantic tokens config");
        }

        let new_formatter_config = self.config.formatter();
        if !old_config.formatter().eq(&new_formatter_config) {
            let enabled = !matches!(new_formatter_config.config, FormatterConfig::Disable);
            self.enable_formatter_caps(enabled)
                .log_error("could not change formatter config");

            self.formatter.change_config(new_formatter_config);
        }

        log::info!("new settings applied");
        Ok(())
    }

    pub(crate) fn did_change_configuration(
        &mut self,
        params: DidChangeConfigurationParams,
    ) -> LspResult<()> {
        // For some clients, we don't get the actual changed configuration and need to
        // poll for it https://github.com/microsoft/language-server-protocol/issues/676
        if let JsonValue::Object(settings) = params.settings {
            return self.on_changed_configuration(settings);
        };

        self.client.send_lsp_request::<WorkspaceConfiguration>(
            ConfigurationParams {
                items: Config::get_items(),
            },
            Self::workspace_configuration_callback,
        );
        Ok(())
    }

    fn workspace_configuration_callback(this: &mut ServerState, resp: sync_ls::lsp::Response) {
        if let Some(err) = resp.error {
            log::error!("failed to request configuration: {err:?}");
            return;
        }

        let Some(result) = resp.result else {
            log::error!("no configuration returned");
            return;
        };

        let Some(resp) = serde_json::from_value::<Vec<JsonValue>>(result)
            .log_error("could not parse configuration")
        else {
            return;
        };
        let _ = this.on_changed_configuration(Config::values_to_map(resp));
    }
}

impl ServerState {
    // todo: handle error
    pub(crate) fn register_capability(&self, registrations: Vec<Registration>) -> Result<()> {
        self.client.send_lsp_request_::<RegisterCapability>(
            RegistrationParams { registrations },
            |_, resp| {
                if let Some(err) = resp.error {
                    log::error!("failed to register capability: {err:?}");
                }
            },
        );
        Ok(())
    }

    pub(crate) fn unregister_capability(
        &self,
        unregisterations: Vec<Unregistration>,
    ) -> Result<()> {
        self.client.send_lsp_request_::<UnregisterCapability>(
            UnregistrationParams { unregisterations },
            |_, resp| {
                if let Some(err) = resp.error {
                    log::error!("failed to unregister capability: {err:?}");
                }
            },
        );
        Ok(())
    }

    /// Registers or unregisters semantic tokens.
    pub(crate) fn enable_sema_token_caps(&mut self, enable: bool) -> Result<()> {
        if !self.const_config().tokens_dynamic_registration {
            log::trace!("skip register semantic by config");
            return Ok(());
        }

        const SEMANTIC_TOKENS_REGISTRATION_ID: &str = "semantic_tokens";
        const SEMANTIC_TOKENS_METHOD_ID: &str = "textDocument/semanticTokens";

        pub fn get_semantic_tokens_registration(options: SemanticTokensOptions) -> Registration {
            Registration {
                id: SEMANTIC_TOKENS_REGISTRATION_ID.to_owned(),
                method: SEMANTIC_TOKENS_METHOD_ID.to_owned(),
                register_options: Some(
                    serde_json::to_value(options)
                        .expect("semantic tokens options should be representable as JSON value"),
                ),
            }
        }

        pub fn get_semantic_tokens_unregistration() -> Unregistration {
            Unregistration {
                id: SEMANTIC_TOKENS_REGISTRATION_ID.to_owned(),
                method: SEMANTIC_TOKENS_METHOD_ID.to_owned(),
            }
        }

        match (enable, self.sema_tokens_registered) {
            (true, false) => {
                log::trace!("registering semantic tokens");
                let options = get_semantic_tokens_options();
                self.register_capability(vec![get_semantic_tokens_registration(options)])
                    .inspect(|_| self.sema_tokens_registered = enable)
                    .context("could not register semantic tokens")
            }
            (false, true) => {
                log::trace!("unregistering semantic tokens");
                self.unregister_capability(vec![get_semantic_tokens_unregistration()])
                    .inspect(|_| self.sema_tokens_registered = enable)
                    .context("could not unregister semantic tokens")
            }
            _ => Ok(()),
        }
    }

    /// Registers or unregisters document formatter.
    pub(crate) fn enable_formatter_caps(&mut self, enable: bool) -> Result<()> {
        if !self.const_config().doc_fmt_dynamic_registration {
            log::trace!("skip dynamic register formatter by config");
            return Ok(());
        }

        const FORMATTING_REGISTRATION_ID: &str = "formatting";
        const DOCUMENT_FORMATTING_METHOD_ID: &str = "textDocument/formatting";

        pub fn get_formatting_registration() -> Registration {
            Registration {
                id: FORMATTING_REGISTRATION_ID.to_owned(),
                method: DOCUMENT_FORMATTING_METHOD_ID.to_owned(),
                register_options: None,
            }
        }

        pub fn get_formatting_unregistration() -> Unregistration {
            Unregistration {
                id: FORMATTING_REGISTRATION_ID.to_owned(),
                method: DOCUMENT_FORMATTING_METHOD_ID.to_owned(),
            }
        }

        match (enable, self.formatter_registered) {
            (true, false) => {
                log::trace!("registering formatter");
                self.register_capability(vec![get_formatting_registration()])
                    .inspect(|_| self.formatter_registered = enable)
                    .context("could not register formatter")
            }
            (false, true) => {
                log::trace!("unregistering formatter");
                self.unregister_capability(vec![get_formatting_unregistration()])
                    .inspect(|_| self.formatter_registered = enable)
                    .context("could not unregister formatter")
            }
            _ => Ok(()),
        }
    }
}
