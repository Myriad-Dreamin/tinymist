use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::Engine;
use futures::future::MaybeDone;
use lsp_types::*;
use reflexo::ImmutPath;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use sync_ls::{Connection, LspClientRoot, LspMessage, Message, ScheduleResult};
use tempfile::TempDir;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::mpsc;
use typst::ecow::EcoString;

use crate::actor::editor::{EditorRequest, ProjVersion};
use crate::input::{FileChange, FileChangeResult, FsChangeParams};
use crate::project::{Interrupt, LspInterrupt, ProjectInsId};
use crate::{CompileFontArgs, Config, ConstConfig, ServerState};

const MAIN: &str = "main.typ";
const DEP: &str = "dep.typ";
const RENAMED_DEP: &str = "renamed.typ";
const CASE_RENAMED_DEP: &str = "Dep.typ";
const NEW_DEP: &str = "new.typ";
const DIR_DEP: &str = "dir/dep.typ";
const RENAMED_DIR_DEP: &str = "renamed-dir/dep.typ";
const OTHER_DEP: &str = "other.typ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiSensitivity {
    GraphDependent,
    SourceLocal,
    Diagnostics,
    SemanticTokens,
    RenameAssistance,
    ShadowOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepresentativeProbe {
    CreateDependency,
    ContentUpdate,
    TransientEmpty,
    ReadErrorRecovery,
    RemoveDependency,
    DeleteThenRecreate,
    AtomicReplace,
    RenameStale,
    RenameUpdated,
    CaseRename,
    RootBoundaryMove,
    DirectoryRenameStale,
    DirectoryRenameUpdated,
    DirectoryDelete,
    DirectoryRootMove,
    MembershipRemoval,
    MembershipAddition,
    ShadowOpenRace,
    SymlinkReadResult,
    MixedBatch,
}

#[derive(Debug)]
struct LspMatrixRow {
    id: &'static str,
    sensitivities: &'static [ApiSensitivity],
    represented_by: RepresentativeProbe,
}

const GRAPH_DIAG: &[ApiSensitivity] =
    &[ApiSensitivity::GraphDependent, ApiSensitivity::Diagnostics];
const GRAPH_SOURCE_SEMANTIC: &[ApiSensitivity] = &[
    ApiSensitivity::GraphDependent,
    ApiSensitivity::SourceLocal,
    ApiSensitivity::SemanticTokens,
];
const DIAG_SEMANTIC: &[ApiSensitivity] =
    &[ApiSensitivity::Diagnostics, ApiSensitivity::SemanticTokens];
const GRAPH_DIAG_RENAME: &[ApiSensitivity] = &[
    ApiSensitivity::GraphDependent,
    ApiSensitivity::Diagnostics,
    ApiSensitivity::RenameAssistance,
];
const SOURCE_SEMANTIC: &[ApiSensitivity] =
    &[ApiSensitivity::SourceLocal, ApiSensitivity::SemanticTokens];
const SHADOW_SOURCE_SEMANTIC: &[ApiSensitivity] = &[
    ApiSensitivity::SourceLocal,
    ApiSensitivity::SemanticTokens,
    ApiSensitivity::ShadowOpen,
];
const ALL_SENSITIVITIES: &[ApiSensitivity] = &[
    ApiSensitivity::GraphDependent,
    ApiSensitivity::SourceLocal,
    ApiSensitivity::Diagnostics,
    ApiSensitivity::SemanticTokens,
    ApiSensitivity::RenameAssistance,
    ApiSensitivity::ShadowOpen,
];

const LSP_WORKSPACE_CHANGE_MATRIX: &[LspMatrixRow] = &[
    LspMatrixRow {
        id: "O01",
        sensitivities: GRAPH_DIAG,
        represented_by: RepresentativeProbe::CreateDependency,
    },
    LspMatrixRow {
        id: "O02",
        sensitivities: GRAPH_SOURCE_SEMANTIC,
        represented_by: RepresentativeProbe::ContentUpdate,
    },
    LspMatrixRow {
        id: "O03",
        sensitivities: DIAG_SEMANTIC,
        represented_by: RepresentativeProbe::TransientEmpty,
    },
    LspMatrixRow {
        id: "O04",
        sensitivities: GRAPH_DIAG,
        represented_by: RepresentativeProbe::ReadErrorRecovery,
    },
    LspMatrixRow {
        id: "O05",
        sensitivities: GRAPH_DIAG,
        represented_by: RepresentativeProbe::RemoveDependency,
    },
    LspMatrixRow {
        id: "O06",
        sensitivities: GRAPH_DIAG,
        represented_by: RepresentativeProbe::DeleteThenRecreate,
    },
    LspMatrixRow {
        id: "O07",
        sensitivities: SOURCE_SEMANTIC,
        represented_by: RepresentativeProbe::AtomicReplace,
    },
    LspMatrixRow {
        id: "O08",
        sensitivities: GRAPH_DIAG_RENAME,
        represented_by: RepresentativeProbe::RenameStale,
    },
    LspMatrixRow {
        id: "O09",
        sensitivities: GRAPH_DIAG_RENAME,
        represented_by: RepresentativeProbe::RenameUpdated,
    },
    LspMatrixRow {
        id: "O10",
        sensitivities: GRAPH_DIAG_RENAME,
        represented_by: RepresentativeProbe::CaseRename,
    },
    LspMatrixRow {
        id: "O11",
        sensitivities: GRAPH_DIAG,
        represented_by: RepresentativeProbe::RootBoundaryMove,
    },
    LspMatrixRow {
        id: "O12",
        sensitivities: GRAPH_DIAG_RENAME,
        represented_by: RepresentativeProbe::DirectoryRenameStale,
    },
    LspMatrixRow {
        id: "O13",
        sensitivities: GRAPH_DIAG_RENAME,
        represented_by: RepresentativeProbe::DirectoryRenameUpdated,
    },
    LspMatrixRow {
        id: "O14",
        sensitivities: GRAPH_DIAG,
        represented_by: RepresentativeProbe::DirectoryDelete,
    },
    LspMatrixRow {
        id: "O15",
        sensitivities: GRAPH_DIAG,
        represented_by: RepresentativeProbe::DirectoryRootMove,
    },
    LspMatrixRow {
        id: "O16",
        sensitivities: GRAPH_DIAG,
        represented_by: RepresentativeProbe::MembershipRemoval,
    },
    LspMatrixRow {
        id: "O17",
        sensitivities: GRAPH_DIAG,
        represented_by: RepresentativeProbe::MembershipAddition,
    },
    LspMatrixRow {
        id: "O18",
        sensitivities: SHADOW_SOURCE_SEMANTIC,
        represented_by: RepresentativeProbe::ShadowOpenRace,
    },
    LspMatrixRow {
        id: "O19",
        sensitivities: GRAPH_DIAG,
        represented_by: RepresentativeProbe::SymlinkReadResult,
    },
    LspMatrixRow {
        id: "O20",
        sensitivities: ALL_SENSITIVITIES,
        represented_by: RepresentativeProbe::MixedBatch,
    },
];

struct LspHarness {
    root: TempDir,
    runtime: Runtime,
    _client_root: LspClientRoot,
    receiver: sync_ls::TConnectionRx<LspMessage>,
    server: ServerState,
    editor_rx: mpsc::UnboundedReceiver<EditorRequest>,
    last_diag_revision: usize,
}

#[derive(Debug)]
struct DiagnosticPublication {
    version: ProjVersion,
    diagnostics: Option<tinymist_query::DiagnosticsMap>,
}

impl LspHarness {
    fn new(files: &[(&str, &str)]) -> Self {
        let root = tempfile::tempdir().expect("failed to create temp workspace");
        for (path, source) in files {
            write_fixture(root.path(), path, source);
        }

        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to create test runtime");
        let connection = Connection::<LspMessage>::channel();
        let receiver = connection.receiver;
        let client_root = LspClientRoot::new(runtime.handle().clone(), connection.sender);
        let client = client_root.weak().to_typed::<ServerState>();
        let (editor_tx, editor_rx) = mpsc::unbounded_channel();

        let root_path: ImmutPath = root.path().into();
        let main_path: ImmutPath = root.path().join(MAIN).as_path().into();
        let mut config = Config::new(
            ConstConfig::default(),
            vec![root_path.clone()],
            CompileFontArgs::default(),
        );
        config.entry_resolver.root_path = Some(root_path);
        config.entry_resolver.entry = Some(main_path);
        config.has_default_entry_path = true;
        config.notify_status = false;

        let mut harness = Self {
            root,
            runtime,
            _client_root: client_root,
            receiver,
            server: ServerState::new(client, config, editor_tx),
            editor_rx,
            last_diag_revision: 0,
        };
        harness
            .server
            .pin_main_file(Some(harness.path(MAIN).as_path().into()))
            .expect("failed to pin main file");
        harness.drain_project_events();
        harness
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.path().join(rel)
    }

    fn uri(&self, rel: &str) -> Url {
        Url::from_file_path(self.path(rel)).expect("fixture path should convert to file URL")
    }

    fn text_document(&self, rel: &str) -> TextDocumentIdentifier {
        TextDocumentIdentifier { uri: self.uri(rel) }
    }

    fn text_position(&self, rel: &str, line: u32, character: u32) -> TextDocumentPositionParams {
        TextDocumentPositionParams {
            text_document: self.text_document(rel),
            position: Position { line, character },
        }
    }

    fn position_of(&self, rel: &str, needle: &str, occurrence: usize, offset: usize) -> Position {
        let source = fs::read_to_string(self.path(rel)).expect("failed to read fixture source");
        position_in(&source, needle, occurrence, offset)
    }

    fn compile_primary(&mut self) {
        self.server
            .project
            .interrupt(Interrupt::Compile(ProjectInsId::PRIMARY));
        self.drain_project_events();
    }

    fn fs_insert(&mut self, rel: &str, source: &str) {
        self.fs_batch(&[(rel, source)], &[], false);
    }

    fn fs_error(&mut self, rel: &str, error: &str) {
        let change = FileChange {
            uri: self.uri(rel).to_string(),
            content: FileChangeResult::Err {
                error: EcoString::from(error),
            },
        };
        let response = self.server.fs_change(FsChangeParams {
            inserts: vec![change],
            removes: vec![],
            is_sync: false,
        });
        self.resolve_json(response);
        self.drain_project_events();
    }

    fn fs_remove(&mut self, rel: &str) {
        self.fs_batch(&[], &[rel], false);
    }

    fn real_fs_insert(&mut self, rel: &str, source: &str) {
        write_fixture(self.root.path(), rel, source);
    }

    fn real_fs_remove(&mut self, rel: &str) {
        remove_existing_fixture(self.root.path(), rel);
    }

    fn fs_batch(&mut self, inserts: &[(&str, &str)], removes: &[&str], is_sync: bool) {
        for rel in removes {
            remove_existing_fixture(self.root.path(), rel);
        }
        for (rel, source) in inserts {
            write_fixture(self.root.path(), rel, source);
        }

        let inserts = inserts
            .iter()
            .map(|(rel, source)| FileChange {
                uri: self.uri(rel).to_string(),
                content: FileChangeResult::Ok {
                    content: base64::engine::general_purpose::STANDARD.encode(source),
                },
            })
            .collect();
        let removes = removes
            .iter()
            .map(|rel| self.uri(rel).to_string())
            .collect();

        let response = self.server.fs_change(FsChangeParams {
            inserts,
            removes,
            is_sync,
        });
        self.resolve_json(response);
        self.drain_project_events();
    }

    fn open_source(&mut self, rel: &str, source: &str) {
        self.server
            .create_source(self.path(rel).as_path().into(), source.to_owned())
            .expect("failed to open source in memory");
        self.drain_project_events();
    }

    fn edit_open_source(&mut self, rel: &str, source: &str) {
        let encoding = self.server.const_config().position_encoding;
        self.server
            .edit_source(
                self.path(rel).as_path().into(),
                vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: source.to_owned(),
                }],
                encoding,
            )
            .expect("failed to edit open source");
        self.drain_project_events();
    }

    fn close_source(&mut self, rel: &str) {
        self.server
            .remove_source(self.path(rel).as_path().into())
            .expect("failed to close source");
        self.drain_project_events();
    }

    fn goto_definition_uris(&mut self, rel: &str, position: Position) -> Vec<Url> {
        let response = self.server.goto_definition(GotoDefinitionParams {
            text_document_position_params: self.text_position(
                rel,
                position.line,
                position.character,
            ),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        });
        let result: Option<GotoDefinitionResponse> = self.decode_egress_response(response);
        match result {
            Some(GotoDefinitionResponse::Scalar(location)) => vec![location.uri],
            Some(GotoDefinitionResponse::Array(locations)) => {
                locations.into_iter().map(|location| location.uri).collect()
            }
            Some(GotoDefinitionResponse::Link(links)) => {
                links.into_iter().map(|link| link.target_uri).collect()
            }
            None => vec![],
        }
    }

    fn reference_uris(&mut self, rel: &str, position: Position) -> Vec<Url> {
        let response = self.server.references(ReferenceParams {
            text_document_position: self.text_position(rel, position.line, position.character),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext {
                include_declaration: true,
            },
        });
        let result: Option<Vec<Location>> = self.decode_egress_response(response);
        result
            .unwrap_or_default()
            .into_iter()
            .map(|location| location.uri)
            .collect()
    }

    fn hover_text(&mut self, rel: &str, position: Position) -> Option<String> {
        let response = self.server.hover(HoverParams {
            text_document_position_params: self.text_position(
                rel,
                position.line,
                position.character,
            ),
            work_done_progress_params: WorkDoneProgressParams::default(),
        });
        let result: Option<Hover> = self.decode_egress_response(response);
        result.map(|hover| match hover.contents {
            HoverContents::Scalar(MarkedString::String(text)) => text,
            HoverContents::Scalar(MarkedString::LanguageString(text)) => text.value,
            HoverContents::Array(items) => items
                .into_iter()
                .map(|item| match item {
                    MarkedString::String(text) => text,
                    MarkedString::LanguageString(text) => text.value,
                })
                .collect::<Vec<_>>()
                .join("\n"),
            HoverContents::Markup(markup) => markup.value,
        })
    }

    fn completion_labels(&mut self, rel: &str, position: Position) -> BTreeSet<String> {
        let response = self.server.completion(CompletionParams {
            text_document_position: self.text_position(rel, position.line, position.character),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: Some(CompletionContext {
                trigger_kind: CompletionTriggerKind::INVOKED,
                trigger_character: None,
            }),
        });
        let result: Option<CompletionList> = self.decode_egress_response(response);
        result
            .unwrap_or_else(|| CompletionList {
                is_incomplete: false,
                items: vec![],
            })
            .items
            .into_iter()
            .map(|item| item.label)
            .collect()
    }

    fn workspace_symbol_uris(&mut self, query: &str) -> Vec<Url> {
        let response = self.server.symbol(WorkspaceSymbolParams {
            query: query.to_owned(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        });
        let result: Option<Vec<SymbolInformation>> = self.decode_egress_response(response);
        result
            .unwrap_or_default()
            .into_iter()
            .map(|symbol| symbol.location.uri)
            .collect()
    }

    fn document_symbol_names(&mut self, rel: &str) -> BTreeSet<String> {
        let response = self.server.document_symbol(DocumentSymbolParams {
            text_document: self.text_document(rel),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        });
        let result: Option<DocumentSymbolResponse> = self.decode_egress_response(response);
        match result {
            Some(DocumentSymbolResponse::Nested(symbols)) => symbols
                .into_iter()
                .flat_map(flatten_document_symbol)
                .collect(),
            Some(DocumentSymbolResponse::Flat(symbols)) => {
                symbols.into_iter().map(|symbol| symbol.name).collect()
            }
            None => BTreeSet::new(),
        }
    }

    fn semantic_full(&mut self, rel: &str) -> SemanticTokens {
        let response = self.server.semantic_tokens_full(SemanticTokensParams {
            text_document: self.text_document(rel),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        });
        let result: Option<SemanticTokensResult> = self.decode_egress_response(response);
        match result.expect("expected semantic token response") {
            SemanticTokensResult::Tokens(tokens) => tokens,
            SemanticTokensResult::Partial(_) => {
                panic!("unexpected partial semantic token response")
            }
        }
    }

    fn semantic_delta_result_id(
        &mut self,
        rel: &str,
        previous_result_id: String,
    ) -> Option<String> {
        let response = self
            .server
            .semantic_tokens_full_delta(SemanticTokensDeltaParams {
                text_document: self.text_document(rel),
                previous_result_id,
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            });
        let result: Option<SemanticTokensFullDeltaResult> = self.decode_egress_response(response);
        match result.expect("expected semantic token delta response") {
            SemanticTokensFullDeltaResult::Tokens(tokens) => tokens.result_id,
            SemanticTokensFullDeltaResult::TokensDelta(delta) => delta.result_id,
            SemanticTokensFullDeltaResult::PartialTokensDelta { .. } => None,
        }
    }

    fn rename_edit(&mut self, rel: &str, position: Position, new_name: &str) -> WorkspaceEdit {
        let response = self.server.rename(RenameParams {
            text_document_position: self.text_position(rel, position.line, position.character),
            new_name: new_name.to_owned(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        });
        let result: Option<WorkspaceEdit> = self.decode_egress_response(response);
        result.expect("expected rename workspace edit")
    }

    fn will_rename_edit(&mut self, old_rel: &str, new_rel: &str) -> Option<WorkspaceEdit> {
        let response = self.server.will_rename_files(RenameFilesParams {
            files: vec![FileRename {
                old_uri: self.uri(old_rel).to_string(),
                new_uri: self.uri(new_rel).to_string(),
            }],
        });
        let result: Option<WorkspaceEdit> = self.decode_egress_response(response);
        result
    }

    fn drain_diagnostic_publications(
        &mut self,
        mut on_publication: impl FnMut(DiagnosticPublication) -> bool,
    ) -> bool {
        let mut matched = false;
        while let Ok(request) = self.editor_rx.try_recv() {
            if let EditorRequest::Diag(version, diagnostics) = request {
                if version.id != ProjectInsId::PRIMARY {
                    continue;
                }
                if version.revision < self.last_diag_revision {
                    continue;
                }
                self.last_diag_revision = version.revision;
                matched |= on_publication(DiagnosticPublication {
                    version,
                    diagnostics,
                });
            }
        }

        matched
    }

    fn take_latest_diagnostics(&mut self) -> Option<DiagnosticPublication> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut latest = None;
        loop {
            self.drain_diagnostic_publications(|publication| {
                latest = Some(publication);
                true
            });

            if latest.is_some() || Instant::now() >= deadline {
                break;
            }

            std::thread::sleep(Duration::from_millis(10));
        }
        latest
    }

    fn assert_real_fs_diagnostics_empty(&mut self) {
        let publication =
            self.wait_for_real_fs_diagnostics("empty main diagnostics", |publication, main_uri| {
                let diagnostics = publication.diagnostics.as_ref();
                diagnostics.is_none_or(|diagnostics| {
                    diagnostics.values().all(|items| items.is_empty())
                        && diagnostics
                            .get(main_uri)
                            .is_none_or(|items| items.is_empty())
                })
            });
        let diagnostics = publication.diagnostics.unwrap_or_default();
        assert!(
            diagnostics.values().all(|items| items.is_empty()),
            "expected real filesystem diagnostics to settle empty at revision {}, got {diagnostics:?}",
            publication.version.revision
        );
    }

    fn assert_real_fs_diagnostics_present(&mut self) {
        let publication = self.wait_for_real_fs_diagnostics(
            "non-empty main diagnostics",
            |publication, main_uri| {
                publication
                    .diagnostics
                    .as_ref()
                    .and_then(|diagnostics| diagnostics.get(main_uri))
                    .is_some_and(|items| !items.is_empty())
            },
        );
        let diagnostics = publication.diagnostics.unwrap_or_default();
        assert!(
            diagnostics
                .get(&self.uri(MAIN))
                .is_some_and(|items| !items.is_empty()),
            "expected real filesystem diagnostics on main at revision {}, got {diagnostics:?}",
            publication.version.revision
        );
    }

    fn wait_for_real_fs_diagnostics(
        &mut self,
        description: &str,
        mut matches: impl FnMut(&DiagnosticPublication, &Url) -> bool,
    ) -> DiagnosticPublication {
        let deadline = Instant::now() + Duration::from_secs(10);
        let main_uri = self.uri(MAIN);
        let mut matched = None;
        let mut last_publication = None;

        loop {
            self.drain_diagnostic_publications(|publication| {
                if matches(&publication, &main_uri) {
                    matched = Some(publication);
                    true
                } else {
                    last_publication = Some(format!("{publication:?}"));
                    false
                }
            });
            if let Some(publication) = matched.take() {
                return publication;
            }

            assert!(
                Instant::now() < deadline,
                "timed out waiting for real filesystem {description}; last diagnostics: {}",
                last_publication.unwrap_or_else(|| "<none>".to_owned())
            );

            self.pump_project_event(Duration::from_millis(20));
        }
    }

    fn assert_latest_diagnostics_empty(&mut self) {
        let publication = self
            .take_latest_diagnostics()
            .expect("expected diagnostics publication");
        let diagnostics = publication.diagnostics.unwrap_or_default();
        let main_uri = self.uri(MAIN);
        assert!(
            diagnostics.values().all(|items| items.is_empty()),
            "expected diagnostics to be empty at revision {}, got {diagnostics:?}",
            publication.version.revision
        );
        assert!(
            diagnostics
                .get(&main_uri)
                .is_none_or(|items| items.is_empty()),
            "expected main file diagnostics to be empty at revision {}, got {:?}",
            publication.version.revision,
            diagnostics.get(&main_uri)
        );
    }

    fn assert_latest_diagnostics_present(&mut self) {
        let publication = self
            .take_latest_diagnostics()
            .expect("expected diagnostics publication");
        let diagnostics = publication.diagnostics.unwrap_or_default();
        let main_uri = self.uri(MAIN);
        let main_diagnostics = diagnostics.get(&main_uri);
        assert!(
            main_diagnostics.is_some_and(|items| !items.is_empty()),
            "expected main file diagnostics to be present at revision {}, got {diagnostics:?}",
            publication.version.revision
        );
    }

    fn resolve_json(&self, response: ScheduleResult) -> JsonValue {
        let response = response.expect("request should be scheduled");
        self.runtime
            .block_on(async move {
                match response {
                    MaybeDone::Done(result) => result,
                    MaybeDone::Future(future) => future.await,
                    MaybeDone::Gone => panic!("response was already consumed"),
                }
            })
            .expect("request should complete successfully")
    }

    fn decode_egress_response<T: DeserializeOwned>(&self, response: ScheduleResult) -> T {
        serde_json::from_value(self.resolve_json(response)).expect("failed to decode LSP response")
    }

    fn pump_project_event(&mut self, timeout: Duration) {
        match self.receiver.event.recv_timeout(timeout) {
            Ok(event) => {
                if let Ok(interrupt) = event.downcast::<LspInterrupt>() {
                    self.server.project.interrupt(*interrupt);
                    return;
                }
                panic!("unexpected server event type");
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {}
        }

        while let Ok(message) = self.receiver.lsp.try_recv() {
            if let Message::Lsp(sync_ls::lsp::Message::Request(request)) = message {
                panic!("unexpected client request during test: {request:?}");
            }
        }
    }

    fn drain_project_events(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        // Dependency sync can enqueue follow-up filesystem interrupts after a
        // compile result; wait for a short quiet period before observing state.
        let quiet_for = Duration::from_millis(100);
        let mut idle_since = None;
        loop {
            match self.receiver.event.recv_timeout(Duration::from_millis(20)) {
                Ok(event) => {
                    if let Ok(interrupt) = event.downcast::<LspInterrupt>() {
                        idle_since = None;
                        self.server.project.interrupt(*interrupt);
                        continue;
                    }
                    panic!("unexpected server event type");
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    if self
                        .server
                        .project
                        .compiler
                        .primary
                        .ext
                        .compiling_since
                        .is_none()
                    {
                        let idle_since = idle_since.get_or_insert_with(Instant::now);
                        if idle_since.elapsed() >= quiet_for {
                            break;
                        }
                        continue;
                    }
                    idle_since = None;
                    assert!(
                        Instant::now() < deadline,
                        "timed out waiting for project compile event"
                    );
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
        }

        while let Ok(message) = self.receiver.lsp.try_recv() {
            if let Message::Lsp(sync_ls::lsp::Message::Request(request)) = message {
                panic!("unexpected client request during test: {request:?}");
            }
        }
    }
}

#[test]
fn lsp_workspace_change_matrix_maps_o01_through_o20() {
    assert_eq!(LSP_WORKSPACE_CHANGE_MATRIX.len(), 20);

    let ids = LSP_WORKSPACE_CHANGE_MATRIX
        .iter()
        .map(|row| row.id)
        .collect::<BTreeSet<_>>();
    let expected = (1..=20)
        .map(|idx| format!("O{idx:02}"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ids,
        expected.iter().map(String::as_str).collect::<BTreeSet<_>>()
    );

    for sensitivity in ALL_SENSITIVITIES {
        assert!(
            LSP_WORKSPACE_CHANGE_MATRIX
                .iter()
                .any(|row| row.sensitivities.contains(sensitivity)),
            "missing sensitivity group {sensitivity:?}"
        );
    }

    for row in LSP_WORKSPACE_CHANGE_MATRIX {
        assert!(!row.sensitivities.is_empty(), "{} has no API group", row.id);
        assert!(
            matches!(
                row.represented_by,
                RepresentativeProbe::CreateDependency
                    | RepresentativeProbe::ContentUpdate
                    | RepresentativeProbe::TransientEmpty
                    | RepresentativeProbe::ReadErrorRecovery
                    | RepresentativeProbe::RemoveDependency
                    | RepresentativeProbe::DeleteThenRecreate
                    | RepresentativeProbe::AtomicReplace
                    | RepresentativeProbe::RenameStale
                    | RepresentativeProbe::RenameUpdated
                    | RepresentativeProbe::CaseRename
                    | RepresentativeProbe::RootBoundaryMove
                    | RepresentativeProbe::DirectoryRenameStale
                    | RepresentativeProbe::DirectoryRenameUpdated
                    | RepresentativeProbe::DirectoryDelete
                    | RepresentativeProbe::DirectoryRootMove
                    | RepresentativeProbe::MembershipRemoval
                    | RepresentativeProbe::MembershipAddition
                    | RepresentativeProbe::ShadowOpenRace
                    | RepresentativeProbe::SymlinkReadResult
                    | RepresentativeProbe::MixedBatch
            ),
            "{} has an undocumented representative probe",
            row.id
        );
    }
}

#[test]
fn o01_create_dependency_refreshes_missing_import_focus_apis() {
    let main = main_source(NEW_DEP, "newer");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_present();

    harness.fs_insert(NEW_DEP, "#let newer = 1\n");
    harness.assert_latest_diagnostics_empty();
    assert_graph_resolves(&mut harness, MAIN, "newer", NEW_DEP);
    assert_main_label_references_current_file(&mut harness);
}

#[test]
fn o02_content_update_refreshes_graph_source_and_semantic_apis() {
    let initial = main_source_with_local(DEP, "alpha", "before_symbol");
    let updated = main_source_with_local(DEP, "beta", "after");
    let mut harness =
        LspHarness::new(&[(MAIN, &initial), (DEP, "#let alpha = 1\n#let beta = 2\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();
    harness.open_source(MAIN, &initial);
    let before_tokens = harness.semantic_full(MAIN);
    let before_id = before_tokens
        .result_id
        .clone()
        .expect("expected semantic token result id");
    harness.close_source(MAIN);

    harness.fs_insert(MAIN, &updated);
    harness.assert_latest_diagnostics_empty();
    harness.open_source(MAIN, &updated);

    assert_graph_resolves(&mut harness, MAIN, "beta", DEP);
    assert_source_symbol_changed(
        &mut harness,
        MAIN,
        "before_symbol",
        "after",
        before_tokens,
        before_id,
    );
}

#[test]
fn o03_transient_empty_dependency_publishes_diagnostics_and_empty_semantics() {
    let main = main_source(DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    harness.fs_insert(DEP, "");
    harness.assert_latest_diagnostics_present();

    assert_graph_does_not_resolve_to(&mut harness, MAIN, "alpha", DEP);
    harness.open_source(DEP, "");
    assert!(!harness.document_symbol_names(DEP).contains("alpha"));
    let empty_tokens = harness.semantic_full(DEP);
    assert!(
        empty_tokens.data.is_empty(),
        "expected empty dependency to produce no semantic token data, got {empty_tokens:?}"
    );
}

#[test]
fn o04_read_error_recovery_clears_diagnostics_and_restores_graph_apis() {
    let main = main_source(DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    harness.fs_error(DEP, "permission denied");
    harness.assert_latest_diagnostics_present();

    harness.fs_insert(DEP, "#let alpha = 4\n");
    harness.assert_latest_diagnostics_empty();
    assert_graph_resolves(&mut harness, MAIN, "alpha", DEP);
}

#[test]
fn o05_remove_dependency_retires_old_path_from_graph_apis() {
    let main = main_source(DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    harness.fs_remove(DEP);
    harness.assert_latest_diagnostics_present();
    assert_graph_does_not_resolve_to(&mut harness, MAIN, "alpha", DEP);
    assert_workspace_symbol_excludes(&mut harness, "alpha", DEP);
}

#[test]
fn o06_delete_then_recreate_restores_dependency_from_fresh_harness_state() {
    let main = main_source(DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    harness.fs_remove(DEP);
    harness.assert_latest_diagnostics_present();
    assert_graph_does_not_resolve_to(&mut harness, MAIN, "alpha", DEP);

    harness.fs_insert(DEP, "#let alpha = 6\n");
    harness.assert_latest_diagnostics_empty();
    assert_graph_resolves(&mut harness, MAIN, "alpha", DEP);
}

#[test]
fn o07_atomic_replace_refreshes_source_local_and_semantic_apis() {
    let initial = local_source("alpha_symbol");
    let replaced = local_source("replacement");
    let mut harness = LspHarness::new(&[(MAIN, &initial)]);
    harness.open_source(MAIN, &initial);
    let before_tokens = harness.semantic_full(MAIN);
    let before_id = before_tokens
        .result_id
        .clone()
        .expect("expected semantic token result id");
    harness.edit_open_source(MAIN, &replaced);
    assert_source_symbol_changed(
        &mut harness,
        MAIN,
        "alpha_symbol",
        "replacement",
        before_tokens,
        before_id,
    );
}

#[test]
fn o08_rename_with_stale_references_keeps_old_path_out_of_graph_apis() {
    let main = main_source(DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    assert_file_rename_assistance(&mut harness, DEP, RENAMED_DEP, "renamed");
    harness.fs_batch(&[(RENAMED_DEP, "#let alpha = 8\n")], &[DEP], false);
    harness.assert_latest_diagnostics_present();

    assert_graph_does_not_resolve_to(&mut harness, MAIN, "alpha", DEP);
    assert_workspace_symbol_excludes(&mut harness, "alpha", DEP);
}

#[test]
fn o09_rename_with_updated_references_follows_new_path_in_graph_apis() {
    let main = main_source(DEP, "alpha");
    let updated_main = main_source(RENAMED_DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    assert_file_rename_assistance(&mut harness, DEP, RENAMED_DEP, "renamed");
    harness.fs_batch(
        &[(RENAMED_DEP, "#let alpha = 9\n"), (MAIN, &updated_main)],
        &[DEP],
        false,
    );
    harness.assert_latest_diagnostics_empty();
    assert_graph_resolves(&mut harness, MAIN, "alpha", RENAMED_DEP);
    assert_workspace_symbol_excludes(&mut harness, "alpha", DEP);
}

#[test]
fn o10_case_rename_updates_references_without_retaining_old_case_path() {
    let main = main_source(DEP, "alpha");
    let updated_main = main_source(CASE_RENAMED_DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    assert_file_rename_assistance(&mut harness, DEP, CASE_RENAMED_DEP, "Dep");
    harness.fs_batch(
        &[
            (CASE_RENAMED_DEP, "#let alpha = 10\n"),
            (MAIN, &updated_main),
        ],
        &[DEP],
        false,
    );
    harness.assert_latest_diagnostics_empty();
    assert_graph_resolves(&mut harness, MAIN, "alpha", CASE_RENAMED_DEP);
    assert_workspace_symbol_excludes(&mut harness, "alpha", DEP);
}

#[test]
fn o11_root_boundary_move_retires_old_nested_path_from_graph_apis() {
    let nested_dep = "nested/dep.typ";
    let main = main_source(nested_dep, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (nested_dep, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    harness.fs_batch(&[(DEP, "#let alpha = 11\n")], &[nested_dep], false);
    harness.assert_latest_diagnostics_present();
    assert_graph_does_not_resolve_to(&mut harness, MAIN, "alpha", nested_dep);
    assert_workspace_symbol_excludes(&mut harness, "alpha", nested_dep);
}

#[test]
fn o12_directory_rename_with_stale_references_retires_old_directory_path() {
    let main = main_source(DIR_DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DIR_DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    assert_directory_rename_assistance(&mut harness, "dir", "renamed-dir");
    harness.fs_batch(&[(RENAMED_DIR_DEP, "#let alpha = 12\n")], &[DIR_DEP], false);
    harness.assert_latest_diagnostics_present();
    assert_graph_does_not_resolve_to(&mut harness, MAIN, "alpha", DIR_DEP);
    assert_workspace_symbol_excludes(&mut harness, "alpha", DIR_DEP);
}

#[test]
fn o13_directory_rename_with_updated_references_follows_new_directory_path() {
    let main = main_source(DIR_DEP, "alpha");
    let updated_main = main_source(RENAMED_DIR_DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DIR_DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    assert_directory_rename_assistance(&mut harness, "dir", "renamed-dir");
    harness.fs_batch(
        &[
            (RENAMED_DIR_DEP, "#let alpha = 13\n"),
            (MAIN, &updated_main),
        ],
        &[DIR_DEP],
        false,
    );
    harness.assert_latest_diagnostics_empty();
    assert_graph_resolves(&mut harness, MAIN, "alpha", RENAMED_DIR_DEP);
    assert_workspace_symbol_excludes(&mut harness, "alpha", DIR_DEP);
}

#[test]
fn o14_directory_delete_reports_missing_dependency_and_clears_old_graph_path() {
    let main = main_source(DIR_DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DIR_DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    harness.fs_remove(DIR_DEP);
    harness.assert_latest_diagnostics_present();
    assert_graph_does_not_resolve_to(&mut harness, MAIN, "alpha", DIR_DEP);
    assert_workspace_symbol_excludes(&mut harness, "alpha", DIR_DEP);
}

#[test]
fn o15_directory_root_move_with_updated_references_follows_root_path() {
    let main = main_source(DIR_DEP, "alpha");
    let updated_main = main_source(DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DIR_DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    harness.fs_batch(
        &[(DEP, "#let alpha = 15\n"), (MAIN, &updated_main)],
        &[DIR_DEP],
        false,
    );
    harness.assert_latest_diagnostics_empty();
    assert_graph_resolves(&mut harness, MAIN, "alpha", DEP);
    assert_workspace_symbol_excludes(&mut harness, "alpha", DIR_DEP);
}

#[test]
fn o16_membership_removal_keeps_server_clean_for_remaining_source_apis() {
    let main = main_source(DEP, "alpha");
    let updated_main = local_source("local_only");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    harness.fs_insert(MAIN, &updated_main);
    harness.assert_latest_diagnostics_empty();
    harness.open_source(MAIN, &updated_main);
    assert_graph_resolves(&mut harness, MAIN, "local_only", MAIN);
    assert!(harness.document_symbol_names(MAIN).contains("local_only"));
    assert_semantic_tokens_nonempty(&mut harness, MAIN);
}

#[test]
fn o17_membership_addition_makes_new_dependency_visible_to_graph_apis() {
    let initial = local_source("local_only");
    let updated_main = main_source(DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &initial), (DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    harness.fs_insert(MAIN, &updated_main);
    harness.assert_latest_diagnostics_empty();
    assert_graph_resolves(&mut harness, MAIN, "alpha", DEP);
    assert_main_label_references_current_file(&mut harness);
}

#[test]
fn o18_shadow_open_keeps_memory_source_until_close_then_uses_filesystem() {
    let disk = local_source("disk_symbol");
    let memory = local_source("memory_symbol");
    let filesystem = local_source("filesystem_symbol");
    let mut harness = LspHarness::new(&[(MAIN, &disk)]);

    harness.open_source(MAIN, &memory);
    assert!(harness
        .document_symbol_names(MAIN)
        .contains("memory_symbol"));
    let memory_tokens = harness.semantic_full(MAIN);

    harness.fs_insert(MAIN, &filesystem);
    assert!(harness
        .document_symbol_names(MAIN)
        .contains("memory_symbol"));
    assert!(!harness
        .document_symbol_names(MAIN)
        .contains("filesystem_symbol"));
    assert_eq!(harness.semantic_full(MAIN).data, memory_tokens.data);

    harness.close_source(MAIN);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();
    assert_definition_resolves(&mut harness, MAIN, "filesystem_symbol", MAIN);
}

#[test]
fn o19_symlink_like_read_result_refreshes_graph_apis_for_link_path() {
    let main = main_source("link.typ", "linked");
    let mut harness = LspHarness::new(&[(MAIN, &main)]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_present();

    let link_kind = create_link_fixture(harness.root.path(), "target.typ", "link.typ");
    #[cfg(unix)]
    assert_eq!(link_kind, LinkFixtureKind::Symlink);
    #[cfg(not(unix))]
    assert_eq!(link_kind, LinkFixtureKind::PlainFileFallback);
    harness.fs_insert("link.typ", "#let linked = 19\n");
    harness.assert_latest_diagnostics_empty();
    assert_graph_resolves(&mut harness, MAIN, "linked", "link.typ");
}

#[test]
fn o20_mixed_batch_exercises_all_focus_api_groups_from_one_fresh_case() {
    let main = main_source_with_local(DEP, "alpha", "before_symbol");
    let updated_main = format!(
        "#set heading(numbering: \"1.\")\n#import \"{RENAMED_DEP}\": alpha\n#import \"{OTHER_DEP}\": beta\n= Main <main-label>\n@main-label\n#let after = 20\n#alpha\n#beta\n#after\n"
    );
    let mut harness = LspHarness::new(&[
        (MAIN, &main),
        (DEP, "#let alpha = 1\n"),
        ("stale.typ", "#let stale = 1\n"),
    ]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();
    harness.open_source(MAIN, &main);
    let before_tokens = harness.semantic_full(MAIN);
    let before_id = before_tokens
        .result_id
        .clone()
        .expect("expected semantic token result id");
    harness.close_source(MAIN);
    assert_file_rename_assistance(&mut harness, DEP, RENAMED_DEP, "renamed");

    harness.fs_batch(
        &[
            (RENAMED_DEP, "#let alpha = 20\n"),
            (OTHER_DEP, "#let beta = 20\n"),
            (MAIN, &updated_main),
        ],
        &[DEP, "stale.typ"],
        false,
    );
    harness.assert_latest_diagnostics_empty();
    harness.open_source(MAIN, &updated_main);

    assert_graph_resolves(&mut harness, MAIN, "alpha", RENAMED_DEP);
    assert_graph_resolves(&mut harness, MAIN, "beta", OTHER_DEP);
    assert_workspace_symbol_excludes(&mut harness, "alpha", DEP);
    assert_source_symbol_changed(
        &mut harness,
        MAIN,
        "before_symbol",
        "after",
        before_tokens,
        before_id,
    );
    assert_main_label_references_current_file(&mut harness);
}

#[test]
#[ignore = "uses the host filesystem watcher; CI runs real_fs_* explicitly"]
fn real_fs_dependency_content_change_publishes_diagnostics_without_lsp_fs_change() {
    let main = main_source(DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    harness.real_fs_insert(DEP, "");
    harness.assert_real_fs_diagnostics_present();

    harness.real_fs_insert(DEP, "#let alpha = 2\n");
    harness.assert_real_fs_diagnostics_empty();
}

#[test]
#[ignore = "uses the host filesystem watcher; CI runs real_fs_* explicitly"]
fn real_fs_dependency_remove_and_recreate_publishes_diagnostics_without_lsp_fs_change() {
    let main = main_source(DEP, "alpha");
    let mut harness = LspHarness::new(&[(MAIN, &main), (DEP, "#let alpha = 1\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    harness.real_fs_remove(DEP);
    harness.assert_real_fs_diagnostics_present();

    harness.real_fs_insert(DEP, "#let alpha = 2\n");
    harness.assert_real_fs_diagnostics_empty();
}

fn assert_graph_resolves(harness: &mut LspHarness, rel: &str, symbol: &str, expected_rel: &str) {
    let usage = format!("#{symbol}");
    let position = harness.position_of(rel, &usage, 0, 2);
    let expected_uri = harness.uri(expected_rel);
    let current_uri = harness.uri(rel);

    let definitions = harness.goto_definition_uris(rel, position);
    assert!(
        definitions.contains(&expected_uri),
        "expected definition of {symbol} in {rel} to include {expected_uri}, got {definitions:?}"
    );

    let references = harness.reference_uris(rel, position);
    assert!(
        references.contains(&expected_uri) || references.contains(&current_uri),
        "expected references for {symbol} to include {expected_uri} or {current_uri}, got {references:?}"
    );

    let hover = harness.hover_text(rel, position);
    assert!(
        hover.as_ref().is_some_and(|text| !text.is_empty()),
        "expected non-empty hover for {symbol}, got {hover:?}"
    );

    let completions = harness.completion_labels(rel, position);
    assert!(
        completions.contains(symbol),
        "expected completion labels at {symbol} to contain {symbol}, got {completions:?}"
    );

    let workspace_symbols = harness.workspace_symbol_uris(symbol);
    assert!(
        workspace_symbols.contains(&expected_uri),
        "expected workspace symbols for {symbol} to contain {expected_uri}, got {workspace_symbols:?}"
    );
}

fn assert_definition_resolves(
    harness: &mut LspHarness,
    rel: &str,
    symbol: &str,
    expected_rel: &str,
) {
    let usage = format!("#{symbol}");
    let position = harness.position_of(rel, &usage, 0, 2);
    let expected_uri = harness.uri(expected_rel);
    let definitions = harness.goto_definition_uris(rel, position);
    assert!(
        definitions.contains(&expected_uri),
        "expected definition of {symbol} in {rel} to include {expected_uri}, got {definitions:?}"
    );
}

fn assert_graph_does_not_resolve_to(
    harness: &mut LspHarness,
    rel: &str,
    symbol: &str,
    retired_rel: &str,
) {
    let usage = format!("#{symbol}");
    let position = harness.position_of(rel, &usage, 0, 2);
    let retired_uri = harness.uri(retired_rel);

    let definitions = harness.goto_definition_uris(rel, position);
    assert!(
        !definitions.contains(&retired_uri),
        "expected definition of {symbol} not to retain {retired_uri}, got {definitions:?}"
    );

    let references = harness.reference_uris(rel, position);
    assert!(
        !references.contains(&retired_uri),
        "expected references for {symbol} not to retain {retired_uri}, got {references:?}"
    );

    let hover = harness.hover_text(rel, position);
    assert!(
        hover
            .as_ref()
            .is_none_or(|text| !text.contains(retired_rel)),
        "expected hover for {symbol} not to mention retired path {retired_rel}, got {hover:?}"
    );

    let _ = harness.completion_labels(rel, position);
}

fn assert_main_label_references_current_file(harness: &mut LspHarness) {
    let label_pos = harness.position_of(MAIN, "@main-label", 0, 2);
    let references = harness.reference_uris(MAIN, label_pos);
    assert!(
        references.contains(&harness.uri(MAIN)),
        "expected label references to include current main file, got {references:?}"
    );
}

fn assert_workspace_symbol_excludes(harness: &mut LspHarness, symbol: &str, rel: &str) {
    let uri = harness.uri(rel);
    let workspace_symbols = harness.workspace_symbol_uris(symbol);
    assert!(
        !workspace_symbols.contains(&uri),
        "expected workspace symbols for {symbol} not to contain {uri}, got {workspace_symbols:?}"
    );
}

fn assert_semantic_tokens_nonempty(harness: &mut LspHarness, rel: &str) {
    let tokens = harness.semantic_full(rel);
    assert!(
        !tokens.data.is_empty(),
        "expected non-empty semantic tokens for {rel}, got {tokens:?}"
    );
    assert!(
        tokens.result_id.is_some(),
        "expected semantic tokens for {rel} to carry a result id"
    );
}

fn assert_source_symbol_changed(
    harness: &mut LspHarness,
    rel: &str,
    old_symbol: &str,
    new_symbol: &str,
    before_tokens: SemanticTokens,
    before_id: String,
) {
    let symbols = harness.document_symbol_names(rel);
    assert!(
        !symbols.contains(old_symbol),
        "expected {rel} document symbols to drop {old_symbol}, got {symbols:?}"
    );
    assert!(
        symbols.contains(new_symbol),
        "expected {rel} document symbols to include {new_symbol}, got {symbols:?}"
    );

    let after_tokens = harness.semantic_full(rel);
    assert_ne!(
        after_tokens.data, before_tokens.data,
        "expected semantic token data to change for {rel}"
    );
    assert_ne!(
        after_tokens.result_id,
        Some(before_id.clone()),
        "expected semantic token result id to change for {rel}"
    );
    let delta_id = harness.semantic_delta_result_id(rel, before_id.clone());
    assert_ne!(
        delta_id,
        Some(before_id),
        "expected semantic token delta to avoid reusing the old result id for {rel}"
    );
}

fn assert_file_rename_assistance(
    harness: &mut LspHarness,
    old_rel: &str,
    new_rel: &str,
    fallback_new_name: &str,
) {
    let edit = harness
        .will_rename_edit(old_rel, new_rel)
        .filter(|edit| workspace_edit_carries_rename_assistance(harness, edit, new_rel))
        .unwrap_or_else(|| {
            let include_path_pos = harness.position_of(MAIN, old_rel, 0, 1);
            harness.rename_edit(MAIN, include_path_pos, fallback_new_name)
        });
    assert_workspace_edit_carries_rename_assistance(harness, &edit, new_rel);
}

fn assert_directory_rename_assistance(harness: &mut LspHarness, old_rel: &str, new_rel: &str) {
    // Do not fall back to textDocument/rename here: renaming an import segment
    // like "dir" is not equivalent to moving the directory.
    if let Some(edit) = harness.will_rename_edit(old_rel, new_rel) {
        assert_workspace_edit_carries_rename_assistance(harness, &edit, new_rel);
    }
}

fn workspace_edit_carries_rename_assistance(
    harness: &LspHarness,
    edit: &WorkspaceEdit,
    expected_rel: &str,
) -> bool {
    let expected_uri = harness.uri(expected_rel);
    applied_workspace_edit_to_main(harness, edit)
        .is_some_and(|after| after.contains(&format!("\"{expected_rel}\"")))
        || workspace_edit_resource_renames(edit).contains(&expected_uri)
}

fn assert_workspace_edit_carries_rename_assistance(
    harness: &LspHarness,
    edit: &WorkspaceEdit,
    expected_rel: &str,
) {
    assert!(
        workspace_edit_carries_rename_assistance(harness, edit, expected_rel),
        "workspace edit should either update main imports or rename a resource to {expected_rel}: {edit:?}"
    );
}

fn workspace_edit_resource_renames(edit: &WorkspaceEdit) -> Vec<Url> {
    let mut renames = Vec::new();
    let Some(DocumentChanges::Operations(operations)) = &edit.document_changes else {
        return renames;
    };

    for operation in operations {
        if let DocumentChangeOperation::Op(ResourceOp::Rename(rename)) = operation {
            renames.push(rename.new_uri.clone());
        }
    }

    renames
}

fn applied_workspace_edit_to_main(harness: &LspHarness, edit: &WorkspaceEdit) -> Option<String> {
    let main_uri = harness.uri(MAIN);
    let edits = workspace_edit_text_edits(edit, &main_uri);
    if edits.is_empty() {
        return None;
    }

    let before = fs::read_to_string(harness.path(MAIN)).expect("failed to read main fixture");
    let after = apply_text_edits(&before, &edits);
    if after == before {
        return None;
    }

    Some(after)
}

fn workspace_edit_text_edits(edit: &WorkspaceEdit, uri: &Url) -> Vec<TextEdit> {
    let mut edits = Vec::new();

    if let Some(changes) = &edit.changes {
        if let Some(uri_edits) = changes.get(uri) {
            edits.extend(uri_edits.iter().cloned());
        }
    }

    if let Some(document_changes) = &edit.document_changes {
        match document_changes {
            DocumentChanges::Edits(document_edits) => {
                for document_edit in document_edits {
                    collect_text_document_edit(uri, document_edit, &mut edits);
                }
            }
            DocumentChanges::Operations(operations) => {
                for operation in operations {
                    if let DocumentChangeOperation::Edit(document_edit) = operation {
                        collect_text_document_edit(uri, document_edit, &mut edits);
                    }
                }
            }
        }
    }

    edits
}

fn collect_text_document_edit(
    uri: &Url,
    document_edit: &TextDocumentEdit,
    edits: &mut Vec<TextEdit>,
) {
    if document_edit.text_document.uri != *uri {
        return;
    }

    for edit in &document_edit.edits {
        match edit {
            OneOf::Left(edit) => edits.push(edit.clone()),
            OneOf::Right(edit) => edits.push(edit.text_edit.clone()),
        }
    }
}

fn apply_text_edits(source: &str, edits: &[TextEdit]) -> String {
    let mut replacements = edits
        .iter()
        .map(|edit| {
            (
                byte_offset(source, edit.range.start),
                byte_offset(source, edit.range.end),
                edit.new_text.clone(),
            )
        })
        .collect::<Vec<_>>();
    replacements.sort_by_key(|(start, end, _)| (*start, *end));

    let mut edited = source.to_owned();
    for (start, end, new_text) in replacements.into_iter().rev() {
        edited.replace_range(start..end, &new_text);
    }

    edited
}

fn byte_offset(source: &str, position: Position) -> usize {
    let mut line = 0;
    let mut line_start = 0;
    for (idx, ch) in source.char_indices() {
        if line == position.line {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = idx + ch.len_utf8();
        }
    }
    assert_eq!(
        line, position.line,
        "position line {} is outside source {source:?}",
        position.line
    );

    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |offset| line_start + offset);
    let line_text = &source[line_start..line_end];
    let mut character = 0;
    for (idx, ch) in line_text.char_indices() {
        if character == position.character {
            return line_start + idx;
        }
        character += 1;
        if character == position.character {
            return line_start + idx + ch.len_utf8();
        }
    }

    assert_eq!(
        character, position.character,
        "position character {} is outside source line {line_text:?}",
        position.character
    );
    line_end
}

fn write_fixture(root: &Path, rel: &str, source: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create fixture directory");
    }
    fs::write(path, source).expect("failed to write fixture");
}

fn remove_existing_fixture(root: &Path, rel: &str) {
    let path = root.join(rel);
    let metadata = fs::symlink_metadata(&path)
        .unwrap_or_else(|err| panic!("expected fixture {path:?} to exist before removal: {err}"));
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path).expect("failed to remove fixture directory");
    } else {
        fs::remove_file(path).expect("failed to remove fixture file");
    }
}

fn position_in(source: &str, needle: &str, occurrence: usize, offset: usize) -> Position {
    let mut start = 0;
    let mut found = None;
    for _ in 0..=occurrence {
        let relative = source[start..]
            .find(needle)
            .unwrap_or_else(|| panic!("needle {needle:?} not found in {source:?}"));
        let absolute = start + relative;
        found = Some(absolute);
        start = absolute + needle.len();
    }

    let target = found.expect("needle should be found") + offset;
    let prefix = &source[..target];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let character = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, last_line)| last_line)
        .chars()
        .count() as u32;
    Position { line, character }
}

fn flatten_document_symbol(symbol: DocumentSymbol) -> Vec<String> {
    let mut names = vec![symbol.name];
    for child in symbol.children.unwrap_or_default() {
        names.extend(flatten_document_symbol(child));
    }
    names
}

fn main_source(dep: &str, symbol: &str) -> String {
    format!(
        "#set heading(numbering: \"1.\")\n#import \"{dep}\": {symbol}\n= Main <main-label>\n@main-label\n#{symbol}\n"
    )
}

fn main_source_with_local(dep: &str, symbol: &str, local: &str) -> String {
    format!(
        "#set heading(numbering: \"1.\")\n#import \"{dep}\": {symbol}\n= Main <main-label>\n@main-label\n#let {local} = 1\n#{symbol}\n#{local}\n"
    )
}

fn local_source(symbol: &str) -> String {
    format!("#let {symbol} = 1\n#{symbol}\n")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkFixtureKind {
    #[cfg(unix)]
    Symlink,
    #[cfg(not(unix))]
    PlainFileFallback,
}

fn create_link_fixture(root: &Path, target: &str, link: &str) -> LinkFixtureKind {
    write_fixture(root, target, "#let linked = 0\n");

    #[cfg(unix)]
    {
        let link_path = root.join(link);
        if let Some(parent) = link_path.parent() {
            fs::create_dir_all(parent).expect("failed to create symlink fixture directory");
        }
        std::os::unix::fs::symlink(target, link_path).expect("failed to create symlink fixture");
        LinkFixtureKind::Symlink
    }

    #[cfg(not(unix))]
    {
        write_fixture(root, link, "#let linked = 0\n");
        LinkFixtureKind::PlainFileFallback
    }
}
