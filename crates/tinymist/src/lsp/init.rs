use lsp_types::*;
use serde_json::json;

use super::*;

// todo: svelte-language-server responds to a Goto Definition request with
// LocationLink[] even if the client does not report the
// textDocument.definition.linkSupport capability.

/// Capability to add valid commands to the arguments.
pub trait AddCommands {
    /// Adds commands to the arguments.
    fn add_commands(&mut self, cmds: &[String]);
}

/// The regular initializer.
pub struct RegularInit {
    /// The connection to the client.
    pub client: TypedLspClient<ServerState>,
    /// The font options for the compiler.
    pub font_opts: CompileFontArgs,
    /// The commands to execute.
    pub exec_cmds: Vec<String>,
}

impl AddCommands for RegularInit {
    fn add_commands(&mut self, cmds: &[String]) {
        self.exec_cmds.extend(cmds.iter().cloned());
    }
}

impl Initializer for RegularInit {
    type I = InitializeParams;
    type S = ServerState;
    /// The [`initialize`] request is the first request sent from the client to
    /// the server.
    ///
    /// [`initialize`]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// This method is guaranteed to only execute once. If the client sends this
    /// request to the server again, the server will respond with JSON-RPC
    /// error code `-32600` (invalid request).
    ///
    /// # Panics
    /// Panics if the const configuration is already initialized.
    /// Panics if the cluster is already initialized.
    ///
    /// # Errors
    /// Errors if the configuration could not be updated.
    fn initialize(self, params: InitializeParams) -> (ServerState, AnySchedulableResponse) {
        let (config, err) = Config::extract_lsp_params(params, self.font_opts);

        let super_init = SuperInit {
            client: self.client,
            exec_cmds: self.exec_cmds,
            config,
            err,
        };

        super_init.initialize(())
    }
}

/// The super LSP initializer.
pub struct SuperInit {
    /// Using the connection to the client.
    pub client: TypedLspClient<ServerState>,
    /// The valid commands for `workspace/executeCommand` requests.
    pub exec_cmds: Vec<String>,
    /// The configuration for the server.
    pub config: Config,
    /// Whether an error occurred before super initialization.
    pub err: Option<ResponseError>,
}

impl AddCommands for SuperInit {
    fn add_commands(&mut self, cmds: &[String]) {
        self.exec_cmds.extend(cmds.iter().cloned());
    }
}

impl Initializer for SuperInit {
    type I = ();
    type S = ServerState;
    fn initialize(self, _params: ()) -> (ServerState, AnySchedulableResponse) {
        let SuperInit {
            client,
            exec_cmds,
            config,
            err,
        } = self;
        let const_config = config.const_config.clone();
        // Bootstrap server
        let service = ServerState::main(client, config, err.is_none());

        if let Some(err) = err {
            return (service, Err(err));
        }

        let semantic_tokens_provider = (!const_config.tokens_dynamic_registration).then(|| {
            SemanticTokensServerCapabilities::SemanticTokensOptions(get_semantic_tokens_options())
        });
        let document_formatting_provider =
            (!const_config.doc_fmt_dynamic_registration).then_some(OneOf::Left(true));

        let file_operations = const_config.notify_will_rename_files.then(|| {
            WorkspaceFileOperationsServerCapabilities {
                will_rename: Some(FileOperationRegistrationOptions {
                    filters: vec![FileOperationFilter {
                        scheme: Some("file".to_string()),
                        pattern: FileOperationPattern {
                            glob: "**/*.typ".to_string(),
                            matches: Some(FileOperationPatternKind::File),
                            options: None,
                        },
                    }],
                }),
                ..WorkspaceFileOperationsServerCapabilities::default()
            }
        });

        let res = InitializeResult {
            capabilities: ServerCapabilities {
                // todo: respect position_encoding
                // position_encoding: Some(cc.position_encoding.into()),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec![
                        String::from("("),
                        String::from(","),
                        String::from(":"),
                    ]),
                    retrigger_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    // Please update the language-configuration.json if you are changing this
                    // setting.
                    trigger_characters: Some(vec![
                        String::from("#"),
                        String::from("("),
                        String::from("<"),
                        String::from(","),
                        String::from("."),
                        String::from(":"),
                        String::from("/"),
                        String::from("\""),
                        String::from("@"),
                    ]),
                    ..CompletionOptions::default()
                }),
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                        ..TextDocumentSyncOptions::default()
                    },
                )),
                semantic_tokens_provider,
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: exec_cmds,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                color_provider: Some(ColorProviderCapability::Simple(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                })),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: None,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations,
                }),
                document_formatting_provider,
                inlay_hint_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),

                experimental: Some(json!({
                  "onEnter": true,
                })),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        };

        let res = serde_json::to_value(res).map_err(|e| invalid_params(e.to_string()));
        (service, just_result(res))
    }
}
