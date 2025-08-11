use std::path::{Path, PathBuf};

use comemo::Track;
use dapts::{CompletionItem, ProcessEventStartMethod, StoppedEventReason, ThreadEventReason};
use reflexo::ImmutPath;
use reflexo_typst::{EntryReader, TaskInputs};
use serde::Deserialize;
use sync_ls::{internal_error, invalid_params, invalid_request, just_ok, SchedulableResponse};
use tinymist_std::error::prelude::*;
use typst::{
    foundations::Repr,
    routines::EvalMode,
    syntax::{LinkedNode, Span},
    World,
};
use typst_shim::syntax::LinkedNodeExt;

use super::*;

impl ServerState {
    /// Called at the end of the configuration sequence.
    /// Indicates that all breakpoints etc. have been sent to the DA and that
    /// the 'launch' can start.
    pub(crate) fn configuration_done(
        &mut self,
        _args: dapts::ConfigurationDoneArguments,
    ) -> SchedulableResponse<()> {
        just_ok(())
    }

    /// Should stop the debug session.
    pub(crate) fn disconnect(
        &mut self,
        _args: dapts::DisconnectArguments,
    ) -> SchedulableResponse<()> {
        let _ = self.debug.session.take();

        just_ok(())
    }

    pub(crate) fn terminate_debug(
        &mut self,
        _args: dapts::TerminateArguments,
    ) -> SchedulableResponse<()> {
        let _ = self.debug.session.take();

        self.client
            .send_dap_event::<dapts::event::Terminated>(dapts::TerminatedEvent { restart: None });

        just_ok(())
    }

    pub(crate) fn terminate_debug_thread(
        &mut self,
        args: dapts::TerminateThreadsArguments,
    ) -> SchedulableResponse<()> {
        if args.thread_ids.as_ref().is_none_or(|id| id.is_empty()) {
            return just_ok(());
        }
        let terminate_thread_ok = args.thread_ids.into_iter().flatten().all(|id| id == 1);
        if terminate_thread_ok {
            let _ = self.debug.session.take();
        }

        just_ok(())
    }

    // cancelRequest

    pub(crate) fn attach_debug(
        &mut self,
        args: dapts::AttachRequestArguments,
    ) -> SchedulableResponse<()> {
        self.launch_debug_(
            dapts::LaunchRequestArguments { raw: args.raw },
            ProcessEventStartMethod::Attach,
        )
    }

    pub(crate) fn launch_debug(
        &mut self,
        args: dapts::LaunchRequestArguments,
    ) -> SchedulableResponse<()> {
        self.launch_debug_(args, ProcessEventStartMethod::Launch)
    }

    pub(crate) fn launch_debug_(
        &mut self,
        args: dapts::LaunchRequestArguments,
        method: ProcessEventStartMethod,
    ) -> SchedulableResponse<()> {
        // wait 1 second until configuration has finished (and configurationDoneRequest
        // has been called) await this._configurationDone.wait(1000);

        // start the program in the runtime
        let args = serde_json::from_value::<LaunchDebugArguments>(args.raw).unwrap();

        let program: ImmutPath = Path::new(&args.program).into();
        let root = Path::new(&args.root).into();
        let input = self.resolve_task(program.clone());
        let entry = self
            .entry_resolver()
            .resolve_with_root(Some(root), Some(program));

        // todo: respect lock file
        let input = TaskInputs {
            entry: Some(entry),
            inputs: input.inputs,
        };

        let snapshot = self.project.snapshot().unwrap().snap.clone().task(input);
        let world = &snapshot.world;

        let main = world
            .main_id()
            .ok_or_else(|| internal_error("No main file found"))?;
        let main_source = world.source(main).map_err(invalid_request)?;
        let main_eof = main_source.text().len();
        let source = main_source.clone();

        self.debug.session = Some(DebugSession {
            config: self.config.const_dap_config.clone(),
            snapshot,
            stop_on_entry: args.stop_on_entry.unwrap_or_default(),
            thread_id: 1,
            // Since we haven't implemented breakpoints, we can only stop intermediately and
            // response completions in repl console.
            source,
            position: main_eof,
        });

        self.client
            .send_dap_event::<dapts::event::Process>(dapts::ProcessEvent {
                name: "typst".into(),
                start_method: Some(method),
                ..dapts::ProcessEvent::default()
            });

        self.client
            .send_dap_event::<dapts::event::Thread>(dapts::ThreadEvent {
                reason: ThreadEventReason::Started,
                thread_id: self.debug.session()?.thread_id,
            });

        // Since we haven't implemented breakpoints, we can only stop intermediately and
        // response completions in repl console.
        let _ = self.debug.session()?.stop_on_entry;
        self.client
            .send_dap_event::<dapts::event::Stopped>(dapts::StoppedEvent {
                all_threads_stopped: Some(true),
                reason: StoppedEventReason::Pause,
                description: Some("Paused at the end of the document".into()),
                thread_id: Some(self.debug.session()?.thread_id),
                hit_breakpoint_ids: None,
                preserve_focus_hint: Some(false),
                text: None,
            });

        just_ok(())
    }

    // customRequest
}

/// This interface describes the mock-debug specific launch attributes
/// (which are not part of the Debug Adapter Protocol).
/// The schema for these attributes lives in the package.json of the mock-debug
/// extension. The interface should always match this schema.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchDebugArguments {
    /// An absolute path to the "program" to debug.
    program: String,
    /// The root directory of the program (used to resolve absolute paths).
    root: String,
    /// Automatically stop target after launch. If not specified, target does
    /// not stop.
    stop_on_entry: Option<bool>,
}

impl ServerState {
    pub(crate) fn debug_threads(
        &mut self,
        _args: (),
    ) -> SchedulableResponse<dapts::ThreadsResponse> {
        just_ok(dapts::ThreadsResponse {
            threads: vec![dapts::Thread {
                id: 1,
                name: "thread 1".into(),
            }],
        })
    }
}

impl ServerState {
    pub(crate) fn evaluate_repl(
        &mut self,
        args: dapts::EvaluateArguments,
    ) -> SchedulableResponse<dapts::EvaluateResponse> {
        let session = self.debug.session()?;
        let world = &session.snapshot.world;
        let library = &world.library;

        let root = session.source.root();
        let span = LinkedNode::new(root)
            .leaf_at_compat(session.position)
            .map(|node| node.span())
            .unwrap_or_else(Span::detached);

        let source = typst_shim::eval::eval_compat(&world, &session.source)
            .map_err(|e| invalid_params(format!("{e:?}")))?;

        let val = typst_shim::eval::eval_string(
            &typst::ROUTINES,
            (world as &dyn World).track(),
            &args.expression,
            span,
            EvalMode::Code,
            source.scope().clone(),
        )
        .map_err(|e| invalid_params(format!("{e:?}")))?;

        just_ok(dapts::EvaluateResponse {
            result: format!("{}", val.repr()),
            ty: Some(format!("{}", val.ty().repr())),
            ..dapts::EvaluateResponse::default()
        })
    }

    pub(crate) fn complete_repl(
        &mut self,
        args: dapts::CompletionsArguments,
    ) -> SchedulableResponse<dapts::CompletionsResponse> {
        let _ = args;
        let session = self
            .debug
            .session
            .as_ref()
            .ok_or_else(|| internal_error("No debug session found"))?;

        just_ok(dapts::CompletionsResponse { targets: vec![] })
    }
}
