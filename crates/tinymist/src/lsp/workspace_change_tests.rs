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

use crate::actor::editor::EditorRequest;
use crate::input::{FileChange, FileChangeResult, FsChangeParams};
use crate::project::{Interrupt, LspInterrupt, ProjectInsId};
use crate::{CompileFontArgs, Config, ConstConfig, ServerState};

const MAIN: &str = "main.typ";
const DEP: &str = "dep.typ";
const RENAMED_DEP: &str = "renamed.typ";

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
        self.compile_primary();
    }

    fn fs_remove(&mut self, rel: &str) {
        self.fs_batch(&[], &[rel], false);
    }

    fn fs_batch(&mut self, inserts: &[(&str, &str)], removes: &[&str], is_sync: bool) {
        for rel in removes {
            remove_fixture(self.root.path(), rel);
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
        self.compile_primary();
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
        let result: Option<GotoDefinitionResponse> = self.decode_response(response);
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
        let result: Option<Vec<Location>> = self.decode_response(response);
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
        let result: Option<Hover> = self.decode_response(response);
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
        let result: Option<CompletionList> = self.decode_response(response);
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
        let result: Option<Vec<SymbolInformation>> = self.decode_response(response);
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
        let result: Option<DocumentSymbolResponse> = self.decode_response(response);
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
        let result: Option<SemanticTokensResult> = self.decode_response(response);
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
        let result: Option<SemanticTokensFullDeltaResult> = self.decode_response(response);
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
        let result: Option<WorkspaceEdit> = self.decode_response(response);
        result.expect("expected rename workspace edit")
    }

    fn will_rename_edit(&mut self, old_rel: &str, new_rel: &str) -> Option<WorkspaceEdit> {
        let response = self.server.will_rename_files(RenameFilesParams {
            files: vec![FileRename {
                old_uri: self.uri(old_rel).to_string(),
                new_uri: self.uri(new_rel).to_string(),
            }],
        });
        let result: Option<WorkspaceEdit> = self.decode_response(response);
        result
    }

    fn take_latest_diagnostics(&mut self) -> Option<tinymist_query::DiagnosticsMap> {
        let mut latest = None;
        while let Ok(request) = self.editor_rx.try_recv() {
            if let EditorRequest::Diag(_, diagnostics) = request {
                latest = diagnostics;
            }
        }
        latest
    }

    fn assert_latest_diagnostics_empty(&mut self) {
        let diagnostics = self
            .take_latest_diagnostics()
            .expect("expected diagnostics publication");
        assert!(
            diagnostics.values().all(|items| items.is_empty()),
            "expected diagnostics to be empty, got {diagnostics:?}"
        );
    }

    fn assert_latest_diagnostics_present(&mut self) {
        let diagnostics = self
            .take_latest_diagnostics()
            .expect("expected diagnostics publication");
        assert!(
            diagnostics.values().any(|items| !items.is_empty()),
            "expected diagnostics to be present, got {diagnostics:?}"
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

    fn decode_response<T: DeserializeOwned>(&self, response: ScheduleResult) -> T {
        serde_json::from_value(self.resolve_json(response)).expect("failed to decode LSP response")
    }

    fn drain_project_events(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.receiver.event.recv_timeout(Duration::from_millis(20)) {
                Ok(event) => {
                    if let Ok(interrupt) = event.downcast::<LspInterrupt>() {
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
                        break;
                    }
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
fn graph_dependent_lsp_responses_follow_create_edit_remove_and_rename_rows() {
    let main = format!(
        "#set heading(numbering: \"1.\")\n#import \"{DEP}\": alpha\n= Main <main-label>\n@main-label\n#alpha\n"
    );
    let mut harness = LspHarness::new(&[(MAIN, &main)]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_present();

    harness.fs_insert(DEP, "#let alpha = 1\n");
    harness.assert_latest_diagnostics_empty();

    let alpha_pos = harness.position_of(MAIN, "#alpha", 0, 2);
    let label_pos = harness.position_of(MAIN, "@main-label", 0, 2);
    let dep_uri = harness.uri(DEP);
    assert!(harness
        .goto_definition_uris(MAIN, alpha_pos)
        .contains(&dep_uri));
    assert!(harness
        .reference_uris(MAIN, label_pos)
        .contains(&harness.uri(MAIN)));
    assert!(harness.hover_text(MAIN, alpha_pos).is_some());
    assert!(harness.completion_labels(MAIN, alpha_pos).contains("alpha"));
    assert!(harness.workspace_symbol_uris("alpha").contains(&dep_uri));

    harness.fs_insert(DEP, "#let alpha = 1\n#let beta = 2\n");
    assert!(harness
        .goto_definition_uris(MAIN, alpha_pos)
        .contains(&dep_uri));

    harness.fs_remove(DEP);
    harness.assert_latest_diagnostics_present();
    assert!(!harness
        .goto_definition_uris(MAIN, alpha_pos)
        .contains(&dep_uri));
    assert!(!harness.workspace_symbol_uris("alpha").contains(&dep_uri));

    harness.fs_batch(&[(DEP, "#let alpha = 3\n")], &[], false);
    harness.assert_latest_diagnostics_empty();
    assert!(harness
        .goto_definition_uris(MAIN, alpha_pos)
        .contains(&dep_uri));

    harness.fs_batch(&[(RENAMED_DEP, "#let alpha = 3\n")], &[DEP], false);
    harness.assert_latest_diagnostics_present();
    assert!(!harness
        .goto_definition_uris(MAIN, alpha_pos)
        .contains(&dep_uri));

    harness.fs_insert(MAIN, &main_source(RENAMED_DEP, "alpha"));
    harness.assert_latest_diagnostics_empty();
    let renamed_uri = harness.uri(RENAMED_DEP);
    let alpha_pos = harness.position_of(MAIN, "#alpha", 0, 2);
    assert!(harness
        .goto_definition_uris(MAIN, alpha_pos)
        .contains(&renamed_uri));
    assert!(!harness.workspace_symbol_uris("alpha").contains(&dep_uri));
    assert!(harness
        .workspace_symbol_uris("alpha")
        .contains(&renamed_uri));
}

#[test]
fn graph_dependent_lsp_responses_follow_directory_and_mixed_batch_rows() {
    let mut harness = LspHarness::new(&[
        (MAIN, main_source("dir/dep.typ", "alpha").as_str()),
        ("dir/dep.typ", "#let alpha = 1\n"),
    ]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    let alpha_pos = harness.position_of(MAIN, "#alpha", 0, 2);
    let old_uri = harness.uri("dir/dep.typ");
    let new_uri = harness.uri("renamed-dir/dep.typ");
    assert!(harness
        .goto_definition_uris(MAIN, alpha_pos)
        .contains(&old_uri));

    harness.fs_batch(
        &[("renamed-dir/dep.typ", "#let alpha = 1\n")],
        &["dir/dep.typ"],
        false,
    );
    harness.assert_latest_diagnostics_present();
    assert!(!harness
        .goto_definition_uris(MAIN, alpha_pos)
        .contains(&old_uri));

    harness.fs_insert(MAIN, &main_source("renamed-dir/dep.typ", "alpha"));
    harness.assert_latest_diagnostics_empty();
    let alpha_pos = harness.position_of(MAIN, "#alpha", 0, 2);
    assert!(harness
        .goto_definition_uris(MAIN, alpha_pos)
        .contains(&new_uri));

    harness.fs_batch(
        &[
            ("new-a.typ", "#let alpha = 10\n"),
            ("new-b.typ", "#let beta = 20\n"),
            (
                MAIN,
                "#import \"new-a.typ\": alpha\n#import \"new-b.typ\": beta\n#alpha\n#beta\n",
            ),
        ],
        &["renamed-dir/dep.typ"],
        false,
    );
    harness.assert_latest_diagnostics_empty();
    let alpha_pos = harness.position_of(MAIN, "#alpha", 0, 2);
    let beta_pos = harness.position_of(MAIN, "#beta", 0, 2);
    assert!(harness
        .goto_definition_uris(MAIN, alpha_pos)
        .contains(&harness.uri("new-a.typ")));
    assert!(harness
        .goto_definition_uris(MAIN, beta_pos)
        .contains(&harness.uri("new-b.typ")));
    assert!(!harness.workspace_symbol_uris("alpha").contains(&new_uri));
}

#[test]
fn source_local_and_semantic_token_responses_use_current_source() {
    let initial = "#let alpha = 1\n#alpha\n";
    let edited = "#let beta = 2\n#beta\n";
    let mut harness = LspHarness::new(&[(MAIN, initial)]);
    harness.open_source(MAIN, initial);

    assert!(harness.document_symbol_names(MAIN).contains("alpha"));
    let first_tokens = harness.semantic_full(MAIN);
    let first_id = first_tokens
        .result_id
        .clone()
        .expect("expected semantic token result id");

    harness.edit_open_source(MAIN, edited);
    assert!(!harness.document_symbol_names(MAIN).contains("alpha"));
    assert!(harness.document_symbol_names(MAIN).contains("beta"));
    let edited_tokens = harness.semantic_full(MAIN);
    assert_ne!(edited_tokens.data, first_tokens.data);
    assert_ne!(edited_tokens.result_id, Some(first_id.clone()));

    let delta_id = harness.semantic_delta_result_id(MAIN, first_id.clone());
    assert_ne!(delta_id, Some(first_id));

    harness.close_source(MAIN);
    harness.fs_batch(&[(MAIN, "#let gamma = 3\n#gamma\n")], &[MAIN], false);
    let replaced_tokens = harness.semantic_full(MAIN);
    assert_ne!(replaced_tokens.data, edited_tokens.data);
}

#[test]
fn diagnostics_publish_and_clear_for_missing_read_error_recovery_and_batches() {
    let mut harness = LspHarness::new(&[
        (MAIN, main_source(DEP, "alpha").as_str()),
        (DEP, "#let alpha = 1\n"),
    ]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    harness.fs_remove(DEP);
    harness.assert_latest_diagnostics_present();
    harness.fs_insert(DEP, "#let alpha = 2\n");
    harness.assert_latest_diagnostics_empty();

    harness.fs_error(DEP, "permission denied");
    harness.assert_latest_diagnostics_present();
    harness.fs_insert(DEP, "#let alpha = 3\n");
    harness.assert_latest_diagnostics_empty();

    harness.fs_batch(&[(RENAMED_DEP, "#let alpha = 4\n")], &[DEP], false);
    harness.assert_latest_diagnostics_present();
    harness.fs_insert(MAIN, &main_source(RENAMED_DEP, "alpha"));
    harness.assert_latest_diagnostics_empty();

    harness.fs_batch(
        &[
            ("other.typ", "#let beta = 5\n"),
            (
                MAIN,
                "#import \"renamed.typ\": alpha\n#import \"other.typ\": beta\n#alpha\n#beta\n",
            ),
        ],
        &[],
        false,
    );
    harness.assert_latest_diagnostics_empty();
}

#[test]
fn assisted_and_unassisted_rename_flows_do_not_retain_old_paths() {
    let include_main = format!("#include \"{DEP}\"\n");
    let mut harness = LspHarness::new(&[(MAIN, &include_main), (DEP, "[dependency]\n")]);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();

    let edit = harness
        .will_rename_edit(DEP, RENAMED_DEP)
        .unwrap_or_else(|| {
            let include_path_pos = harness.position_of(MAIN, DEP, 0, 1);
            harness.rename_edit(MAIN, include_path_pos, "renamed")
        });
    let edit_json = serde_json::to_string(&edit).expect("failed to serialize workspace edit");
    assert!(
        edit_json.contains(RENAMED_DEP),
        "workspace edit should update imports to {RENAMED_DEP}: {edit_json}"
    );

    harness.fs_batch(&[(RENAMED_DEP, "[dependency]\n")], &[DEP], false);
    harness.assert_latest_diagnostics_present();

    harness.fs_insert(MAIN, &format!("#include \"{RENAMED_DEP}\"\n"));
    harness.assert_latest_diagnostics_empty();
}

#[test]
fn shadow_open_files_use_memory_until_close_then_current_filesystem() {
    let disk = "#let disk = 1\n#disk\n";
    let memory = "#let memory = 2\n#memory\n";
    let filesystem = "#let filesystem = 3\n#filesystem\n";
    let mut harness = LspHarness::new(&[(MAIN, disk)]);

    harness.open_source(MAIN, memory);
    assert!(harness.document_symbol_names(MAIN).contains("memory"));

    harness.fs_insert(MAIN, filesystem);
    assert!(harness.document_symbol_names(MAIN).contains("memory"));
    assert!(!harness.document_symbol_names(MAIN).contains("filesystem"));

    harness.close_source(MAIN);
    harness.compile_primary();
    harness.assert_latest_diagnostics_empty();
    let filesystem_pos = harness.position_of(MAIN, "#filesystem", 0, 2);
    assert!(harness
        .goto_definition_uris(MAIN, filesystem_pos)
        .contains(&harness.uri(MAIN)));
}

fn write_fixture(root: &Path, rel: &str, source: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create fixture directory");
    }
    fs::write(path, source).expect("failed to write fixture");
}

fn remove_fixture(root: &Path, rel: &str) {
    let path = root.join(rel);
    if path.is_dir() {
        fs::remove_dir_all(path).expect("failed to remove fixture directory");
    } else if path.exists() {
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
    format!("#import \"{dep}\": {symbol}\n#{symbol}\n")
}
