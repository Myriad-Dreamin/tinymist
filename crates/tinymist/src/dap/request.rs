use std::{path::Path, sync::Arc};

use dapts::{ProcessEventStartMethod, ThreadEventReason};
use reflexo::ImmutPath;
use reflexo_typst::{EntryReader, TaskInputs};
use serde::Deserialize;
use sync_ls::{
    internal_error, invalid_request, just_ok, RequestId, SchedulableResponse, ScheduledResult,
};
use tinymist_dap::DebugRequest;
use tinymist_std::error::prelude::*;
use typst::World;

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

        let (adaptor_tx, adaptor_rx) = std::sync::mpsc::channel();
        let adaptor = Arc::new(Debuggee {
            tx: adaptor_tx,
            stop_on_entry: args.stop_on_entry.unwrap_or_default(),
            thread_id: 1,
            client: self.client.clone().to_untyped(),
        });

        tinymist_dap::start_session(snapshot.world.clone(), adaptor.clone(), adaptor_rx);

        self.debug.session = Some(DebugSession {
            config: self.config.const_dap_config.clone(),
            adaptor,
            snapshot,
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
                thread_id: self.debug.session()?.adaptor.thread_id,
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
        req_id: RequestId,
        args: dapts::EvaluateArguments,
    ) -> ScheduledResult {
        let session = self.debug.session()?;

        session
            .adaptor
            .tx
            .send(DebugRequest::Evaluate(
                RequestId::dap(req_id),
                args.expression,
            ))
            .map_err(|_| internal_error("debug session is closed"))?;
        Ok(Some(()))
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
