use lsp_types::*;
use request::{RegisterCapability, UnregisterCapability};
use sync_lsp::*;
use tinymist_std::error::{prelude::*, IgnoreLogging};

use crate::{init::*, *};

impl ServerState {
    // todo: handle error
    pub(crate) fn register_capability(&self, registrations: Vec<Registration>) -> Result<()> {
        self.client.send_request_::<RegisterCapability>(
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
        self.client.send_request_::<UnregisterCapability>(
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
