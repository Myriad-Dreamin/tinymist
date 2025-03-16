use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use comemo::Track;
use dapts::{CompletionItem, ProcessEventStartMethod, StoppedEventReason, ThreadEventReason};
use reflexo::ImmutPath;
use reflexo_typst::{EntryReader, TaskInputs, TypstPagedDocument};
use serde::Deserialize;
use sync_ls::{
    internal_error, invalid_params, invalid_request, just_ok, RequestId, SchedulableResponse,
    ScheduledResult,
};
use tinymist_dap::{BreakpointContext, DebugAdaptor, DebugRequest};
use tinymist_std::error::prelude::*;
use typst::{
    diag::{SourceResult, Warned},
    foundations::{Repr, Value},
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

        let snapshot = self.project.snapshot().unwrap().task(input);
        let world = &snapshot.world;

        let main = world
            .main_id()
            .ok_or_else(|| internal_error("No main file found"))?;
        let main_source = world.source(main).map_err(invalid_request)?;
        let main_eof = main_source.text().len();
        let source = main_source.clone();

        let (adaptor_tx, adaptor_rx) = std::sync::mpsc::channel();
        let adaptor = Arc::new(Debugee {
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
    //     protected setFunctionBreakPointsRequest(
    //       response: DebugProtocol.SetFunctionBreakpointsResponse,
    //       args: DebugProtocol.SetFunctionBreakpointsArguments,
    //       request?: DebugProtocol.Request,
    //     ): void {
    //       this.sendResponse(response);
    //     }

    //     protected async setBreakPointsRequest(
    //       response: DebugProtocol.SetBreakpointsResponse,
    //       args: DebugProtocol.SetBreakpointsArguments,
    //     ): Promise<void> {
    //       const path = args.source.path as string;
    //       const clientLines = args.lines || [];

    //       // clear all breakpoints for this file
    //       this._runtime.clearBreakpoints(path);

    //       // set and verify breakpoint locations
    //       const actualBreakpoints0 = clientLines.map(async (l) => {
    //         const { verified, line, id } = await this._runtime.setBreakPoint(
    //           path,
    //           this.convertClientLineToDebugger(l),
    //         );
    //         const bp = new Breakpoint(
    //           verified,
    //           this.convertDebuggerLineToClient(line),
    //         ) as DebugProtocol.Breakpoint;
    //         bp.id = id;
    //         return bp;
    //       });
    //       const actualBreakpoints = await
    // Promise.all<DebugProtocol.Breakpoint>(actualBreakpoints0);

    //       // send back the actual breakpoint positions
    //       response.body = {
    //         breakpoints: actualBreakpoints,
    //       };
    //       this.sendResponse(response);
    //     }

    //     protected breakpointLocationsRequest(
    //       response: DebugProtocol.BreakpointLocationsResponse,
    //       args: DebugProtocol.BreakpointLocationsArguments,
    //       request?: DebugProtocol.Request,
    //     ): void {
    //       if (args.source.path) {
    //         const bps = this._runtime.getBreakpoints(
    //           args.source.path,
    //           this.convertClientLineToDebugger(args.line),
    //         );
    //         response.body = {
    //           breakpoints: bps.map((col) => {
    //             return {
    //               line: args.line,
    //               column: this.convertDebuggerColumnToClient(col),
    //             };
    //           }),
    //         };
    //       } else {
    //         response.body = {
    //           breakpoints: [],
    //         };
    //       }
    //       this.sendResponse(response);
    //     }

    //     protected async setExceptionBreakPointsRequest(
    //       response: DebugProtocol.SetExceptionBreakpointsResponse,
    //       args: DebugProtocol.SetExceptionBreakpointsArguments,
    //     ): Promise<void> {
    //       let namedException: string | undefined = undefined;
    //       let otherExceptions = false;

    //       if (args.filterOptions) {
    //         for (const filterOption of args.filterOptions) {
    //           switch (filterOption.filterId) {
    //             case "namedException":
    //               namedException = args.filterOptions[0].condition;
    //               break;
    //             case "otherExceptions":
    //               otherExceptions = true;
    //               break;
    //           }
    //         }
    //       }

    //       if (args.filters) {
    //         if (args.filters.indexOf("otherExceptions") >= 0) {
    //           otherExceptions = true;
    //         }
    //       }

    //       this._runtime.setExceptionsFilters(namedException, otherExceptions);

    //       this.sendResponse(response);
    //     }
}

impl ServerState {
    //     protected exceptionInfoRequest(
    //       response: DebugProtocol.ExceptionInfoResponse,
    //       args: DebugProtocol.ExceptionInfoArguments,
    //     ) {
    //       response.body = {
    //         exceptionId: "Exception ID",
    //         description: "This is a descriptive description of the exception.",
    //         breakMode: "always",
    //         details: {
    //           message: "Message contained in the exception.",
    //           typeName: "Short type name of the exception object",
    //           stackTrace: "stack frame 1\nstack frame 2",
    //         },
    //       };
    //       this.sendResponse(response);
    //     }

    //     protected setDataBreakpointsRequest(
    //       response: DebugProtocol.SetDataBreakpointsResponse,
    //       args: DebugProtocol.SetDataBreakpointsArguments,
    //     ): void {
    //       // clear all data breakpoints
    //       this._runtime.clearAllDataBreakpoints();

    //       response.body = {
    //         breakpoints: [],
    //       };

    //       for (const dbp of args.breakpoints) {
    //         const ok = this._runtime.setDataBreakpoint(dbp.dataId, dbp.accessType
    // || "write");         response.body.breakpoints.push({
    //           verified: ok,
    //         });
    //       }

    //       this.sendResponse(response);
    //     }

    //     protected dataBreakpointInfoRequest(
    //       response: DebugProtocol.DataBreakpointInfoResponse,
    //       args: DebugProtocol.DataBreakpointInfoArguments,
    //     ): void {
    //       response.body = {
    //         dataId: null,
    //         description: "cannot break on data access",
    //         accessTypes: undefined,
    //         canPersist: false,
    //       };

    //       if (args.variablesReference && args.name) {
    //         const v = this._variableHandles.get(args.variablesReference);
    //         if (v === "globals") {
    //           response.body.dataId = args.name;
    //           response.body.description = args.name;
    //           response.body.accessTypes = ["write"];
    //           response.body.canPersist = true;
    //         } else {
    //           response.body.dataId = args.name;
    //           response.body.description = args.name;
    //           response.body.accessTypes = ["read", "write", "readWrite"];
    //           response.body.canPersist = true;
    //         }
    //       }

    //       this.sendResponse(response);
    //     }

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

    //     protected stackTraceRequest(
    //       response: DebugProtocol.StackTraceResponse,
    //       args: DebugProtocol.StackTraceArguments,
    //     ): void {
    //       const startFrame = typeof args.startFrame === "number" ?
    // args.startFrame : 0;       const maxLevels = typeof args.levels === "number"
    // ? args.levels : 1000;       const endFrame = startFrame + maxLevels;

    //       const stk = this._runtime.stack(startFrame, endFrame);

    //       response.body = {
    //         stackFrames: stk.frames.map((f, ix) => {
    //           const sf: DebugProtocol.StackFrame = new StackFrame(
    //             f.index,
    //             f.name,
    //             this.createSource(f.file),
    //             this.convertDebuggerLineToClient(f.line),
    //           );
    //           if (typeof f.column === "number") {
    //             sf.column = this.convertDebuggerColumnToClient(f.column);
    //           }
    //           if (typeof f.instruction === "number") {
    //             const address = this.formatAddress(f.instruction);
    //             sf.name = `${f.name} ${address}`;
    //             sf.instructionPointerReference = address;
    //           }

    //           return sf;
    //         }),
    //         // 4 options for 'totalFrames':
    //         //omit totalFrames property: 	// VS Code has to probe/guess. Should
    // result in a max. of two requests         totalFrames: stk.count, // stk.count
    // is the correct size, should result in a max. of two requests
    // //totalFrames: 1000000 			// not the correct size, should result in a max. of
    // two requests         //totalFrames: endFrame + 20 	// dynamically increases
    // the size with every requested chunk, results in paging       };
    //       this.sendResponse(response);
    //     }
}

impl ServerState {
    //     protected continueRequest(
    //       response: DebugProtocol.ContinueResponse,
    //       args: DebugProtocol.ContinueArguments,
    //     ): void {
    //       this._runtime.continue(false);
    //       this.sendResponse(response);
    //     }

    //     protected reverseContinueRequest(
    //       response: DebugProtocol.ReverseContinueResponse,
    //       args: DebugProtocol.ReverseContinueArguments,
    //     ): void {
    //       this._runtime.continue(true);
    //       this.sendResponse(response);
    //     }

    //     protected nextRequest(
    //       response: DebugProtocol.NextResponse,
    //       args: DebugProtocol.NextArguments,
    //     ): void {
    //       this._runtime.step(args.granularity === "instruction", false);
    //       this.sendResponse(response);
    //     }

    //     protected stepBackRequest(
    //       response: DebugProtocol.StepBackResponse,
    //       args: DebugProtocol.StepBackArguments,
    //     ): void {
    //       this._runtime.step(args.granularity === "instruction", true);
    //       this.sendResponse(response);
    //     }

    //     protected stepInTargetsRequest(
    //       response: DebugProtocol.StepInTargetsResponse,
    //       args: DebugProtocol.StepInTargetsArguments,
    //     ) {
    //       const targets = this._runtime.getStepInTargets(args.frameId);
    //       response.body = {
    //         targets: targets.map((t) => {
    //           return { id: t.id, label: t.label };
    //         }),
    //       };
    //       this.sendResponse(response);
    //     }

    //     protected stepInRequest(
    //       response: DebugProtocol.StepInResponse,
    //       args: DebugProtocol.StepInArguments,
    //     ): void {
    //       this._runtime.stepIn(args.targetId);
    //       this.sendResponse(response);
    //     }

    //     protected stepOutRequest(
    //       response: DebugProtocol.StepOutResponse,
    //       args: DebugProtocol.StepOutArguments,
    //     ): void {
    //       this._runtime.stepOut();
    //       this.sendResponse(response);
    //     }
}

impl ServerState {
    //     protected scopesRequest(
    //       response: DebugProtocol.ScopesResponse,
    //       args: DebugProtocol.ScopesArguments,
    //     ): void {
    //       response.body = {
    //         scopes: [
    //           new Scope("Locals", this._variableHandles.create("locals"), false),
    //           new Scope("Globals", this._variableHandles.create("globals"),
    // true),         ],
    //       };
    //       this.sendResponse(response);
    //     }

    //     protected async writeMemoryRequest(
    //       response: DebugProtocol.WriteMemoryResponse,
    //       { data, memoryReference, offset = 0 }:
    // DebugProtocol.WriteMemoryArguments,     ) {
    //       const variable = this._variableHandles.get(Number(memoryReference));
    //       if (typeof variable === "object") {
    //         const decoded = base64.toByteArray(data);
    //         variable.setMemory(decoded, offset);
    //         response.body = { bytesWritten: decoded.length };
    //       } else {
    //         response.body = { bytesWritten: 0 };
    //       }

    //       this.sendResponse(response);
    //       this.sendEvent(new InvalidatedEvent(["variables"]));
    //     }

    //     protected async readMemoryRequest(
    //       response: DebugProtocol.ReadMemoryResponse,
    //       { offset = 0, count, memoryReference }:
    // DebugProtocol.ReadMemoryArguments,     ) {
    //       const variable = this._variableHandles.get(Number(memoryReference));
    //       if (typeof variable === "object" && variable.memory) {
    //         const memory = variable.memory.subarray(
    //           Math.min(offset, variable.memory.length),
    //           Math.min(offset + count, variable.memory.length),
    //         );

    //         response.body = {
    //           address: offset.toString(),
    //           data: base64.fromByteArray(memory),
    //           unreadableBytes: count - memory.length,
    //         };
    //       } else {
    //         response.body = {
    //           address: offset.toString(),
    //           data: "",
    //           unreadableBytes: count,
    //         };
    //       }

    //       this.sendResponse(response);
    //     }

    //     protected async variablesRequest(
    //       response: DebugProtocol.VariablesResponse,
    //       args: DebugProtocol.VariablesArguments,
    //       request?: DebugProtocol.Request,
    //     ): Promise<void> {
    //       let vs: RuntimeVariable[] = [];

    //       const v = this._variableHandles.get(args.variablesReference);
    //       if (v === "locals") {
    //         vs = this._runtime.getLocalVariables();
    //       } else if (v === "globals") {
    //         if (request) {
    //           this._cancellationTokens.set(request.seq, false);
    //           vs = await this._runtime.getGlobalVariables(
    //             () => !!this._cancellationTokens.get(request.seq),
    //           );
    //           this._cancellationTokens.delete(request.seq);
    //         } else {
    //           vs = await this._runtime.getGlobalVariables();
    //         }
    //       } else if (v && Array.isArray(v.value)) {
    //         vs = v.value;
    //       }

    //       response.body = {
    //         variables: vs.map((v) => this.convertFromRuntime(v)),
    //       };
    //       this.sendResponse(response);
    //     }

    //     protected setVariableRequest(
    //       response: DebugProtocol.SetVariableResponse,
    //       args: DebugProtocol.SetVariableArguments,
    //     ): void {
    //       const container = this._variableHandles.get(args.variablesReference);
    //       const rv =
    //         container === "locals"
    //           ? this._runtime.getLocalVariable(args.name)
    //           : container instanceof RuntimeVariable && container.value
    // instanceof Array             ? container.value.find((v) => v.name ===
    // args.name)             : undefined;

    //       if (rv) {
    //         rv.value = this.convertToRuntime(args.value);
    //         response.body = this.convertFromRuntime(rv);

    //         if (rv.memory && rv.reference) {
    //           this.sendEvent(new MemoryEvent(String(rv.reference), 0,
    // rv.memory.length));         }
    //       }

    //       this.sendResponse(response);
    //     }

    //     protected setExpressionRequest(
    //       response: DebugProtocol.SetExpressionResponse,
    //       args: DebugProtocol.SetExpressionArguments,
    //     ): void {
    //       if (args.expression.startsWith("$")) {
    //         const rv = this._runtime.getLocalVariable(args.expression.substr(1));
    //         if (rv) {
    //           rv.value = this.convertToRuntime(args.value);
    //           response.body = this.convertFromRuntime(rv);
    //           this.sendResponse(response);
    //         } else {
    //           this.sendErrorResponse(response, {
    //             id: 1002,
    //             format: `variable '{lexpr}' not found`,
    //             variables: { lexpr: args.expression },
    //             showUser: true,
    //           });
    //         }
    //       } else {
    //         this.sendErrorResponse(response, {
    //           id: 1003,
    //           format: `'{lexpr}' not an assignable expression`,
    //           variables: { lexpr: args.expression },
    //           showUser: true,
    //         });
    //       }
    //     }
}

impl ServerState {
    //     protected async evaluateRequest(
    //       response: DebugProtocol.EvaluateResponse,
    //       args: DebugProtocol.EvaluateArguments,
    //     ): Promise<void> {
    //       let reply: string | undefined;
    //       let rv: RuntimeVariable | undefined;

    //       switch (args.context) {
    //         case "repl":
    //           // handle some REPL commands:
    //           // 'evaluate' supports to create and delete breakpoints from the
    // 'repl':           const matches = /new +([0-9]+)/.exec(args.expression);
    //           if (matches && matches.length === 2) {
    //             const mbp = await this._runtime.setBreakPoint(
    //               this._runtime.sourceFile,
    //               this.convertClientLineToDebugger(parseInt(matches[1])),
    //             );
    //             const bp = new Breakpoint(
    //               mbp.verified,
    //               this.convertDebuggerLineToClient(mbp.line),
    //               undefined,
    //               this.createSource(this._runtime.sourceFile),
    //             ) as DebugProtocol.Breakpoint;
    //             bp.id = mbp.id;
    //             this.sendEvent(new BreakpointEvent("new", bp));
    //             reply = `breakpoint created`;
    //           } else {
    //             const matches = /del +([0-9]+)/.exec(args.expression);
    //             if (matches && matches.length === 2) {
    //               const mbp = this._runtime.clearBreakPoint(
    //                 this._runtime.sourceFile,
    //                 this.convertClientLineToDebugger(parseInt(matches[1])),
    //               );
    //               if (mbp) {
    //                 const bp = new Breakpoint(false) as DebugProtocol.Breakpoint;
    //                 bp.id = mbp.id;
    //                 this.sendEvent(new BreakpointEvent("removed", bp));
    //                 reply = `breakpoint deleted`;
    //               }
    //             } else {
    //               const matches = /progress/.exec(args.expression);
    //               if (matches && matches.length === 1) {
    //                 if (this._reportProgress) {
    //                   reply = `progress started`;
    //                   this.progressSequence();
    //                 } else {
    //                   reply = `frontend doesn't support progress (capability
    // 'supportsProgressReporting' not set)`;                 }
    //               }
    //             }
    //           }
    //         // fall through

    //         default:
    //           if (args.expression.startsWith("$")) {
    //             rv = this._runtime.getLocalVariable(args.expression.substr(1));
    //           } else {
    //             rv = new RuntimeVariable("eval",
    // this.convertToRuntime(args.expression));           }
    //           break;
    //       }

    //       if (rv) {
    //         const v = this.convertFromRuntime(rv);
    //         response.body = {
    //           result: v.value,
    //           type: v.type,
    //           variablesReference: v.variablesReference,
    //           presentationHint: v.presentationHint,
    //         };
    //       } else {
    //         response.body = {
    //           result: reply ? reply : `evaluate(context: '${args.context}',
    // '${args.expression}')`,           variablesReference: 0,
    //         };
    //       }

    //       this.sendResponse(response);
    //     }

    //     protected completionsRequest(
    //       response: DebugProtocol.CompletionsResponse,
    //       args: DebugProtocol.CompletionsArguments,
    //     ): void {
    //       response.body = {
    //         targets: [
    //           {
    //             label: "item 10",
    //             sortText: "10",
    //           },
    //           {
    //             label: "item 1",
    //             sortText: "01",
    //             detail: "detail 1",
    //           },
    //           {
    //             label: "item 2",
    //             sortText: "02",
    //             detail: "detail 2",
    //           },
    //           {
    //             label: "array[]",
    //             selectionStart: 6,
    //             sortText: "03",
    //           },
    //           {
    //             label: "func(arg)",
    //             selectionStart: 5,
    //             selectionLength: 3,
    //             sortText: "04",
    //           },
    //         ],
    //       };
    //       this.sendResponse(response);
    //     }

    pub(crate) fn evaluate_repl(
        &mut self,
        req_id: RequestId,
        args: dapts::EvaluateArguments,
    ) -> ScheduledResult {
        let session = self.debug.session()?;

        session.adaptor.tx.send(DebugRequest::Evaluate(
            RequestId::dap(req_id),
            args.expression,
        ));
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

        just_ok(dapts::CompletionsResponse {
            targets: vec![
                // CompletionItem {
                //     label: "std".into(),
                //     detail: Some("global module".into()),
                //     length: None,
                //     selection_length: None,
                //     selection_start: None,
                //     sort_text: None,
                //     start: None,
                //     text: None,
                //     ty: None,
                // }
            ],
        })
    }
}

//     //---- helpers

//     private convertToRuntime(value: string): IRuntimeVariableType {
//       value = value.trim();

//       if (value === "true") {
//         return true;
//       }
//       if (value === "false") {
//         return false;
//       }
//       if (value[0] === "'" || value[0] === '"') {
//         return value.substr(1, value.length - 2);
//       }
//       const n = parseFloat(value);
//       if (!isNaN(n)) {
//         return n;
//       }
//       return value;
//     }

//     private convertFromRuntime(v: RuntimeVariable): DebugProtocol.Variable {
//       let dapVariable: DebugProtocol.Variable = {
//         name: v.name,
//         value: "???",
//         type: typeof v.value,
//         variablesReference: 0,
//         evaluateName: "$" + v.name,
//       };

//       if (v.name.indexOf("lazy") >= 0) {
//         // a "lazy" variable needs an additional click to retrieve its value

//         dapVariable.value = "lazy var"; // placeholder value
//         v.reference ??= this._variableHandles.create(
//           new RuntimeVariable("", [new RuntimeVariable("", v.value)]),
//         );
//         dapVariable.variablesReference = v.reference;
//         dapVariable.presentationHint = { lazy: true };
//       } else {
//         if (Array.isArray(v.value)) {
//           dapVariable.value = "Object";
//           v.reference ??= this._variableHandles.create(v);
//           dapVariable.variablesReference = v.reference;
//         } else {
//           switch (typeof v.value) {
//             case "number":
//               if (Math.round(v.value) === v.value) {
//                 dapVariable.value = this.formatNumber(v.value);
//                 (<any>dapVariable).__vscodeVariableMenuContext = "simple"; //
// enable context menu contribution                 dapVariable.type =
// "integer";               } else {
//                 dapVariable.value = v.value.toString();
//                 dapVariable.type = "float";
//               }
//               break;
//             case "string":
//               dapVariable.value = `"${v.value}"`;
//               break;
//             case "boolean":
//               dapVariable.value = v.value ? "true" : "false";
//               break;
//             default:
//               dapVariable.value = typeof v.value;
//               break;
//           }
//         }
//       }

//       if (v.memory) {
//         v.reference ??= this._variableHandles.create(v);
//         dapVariable.memoryReference = String(v.reference);
//       }

//       return dapVariable;
//     }

//     private formatAddress(x: number, pad = 8) {
//       return "mem" + (this._addressesInHex ? "0x" +
// x.toString(16).padStart(8, "0") : x.toString(10));     }

//     private formatNumber(x: number) {
//       return this._valuesInHex ? "0x" + x.toString(16) : x.toString(10);
//     }

//     private createSource(filePath: string): Source {
//       return new Source(
//         basename(filePath),
//         this.convertDebuggerPathToClient(filePath),
//         undefined,
//         undefined,
//         "mock-adapter-data",
//       );
//     }
//   }
