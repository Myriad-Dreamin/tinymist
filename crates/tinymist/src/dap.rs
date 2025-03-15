// /*---------------------------------------------------------
//  * Copyright (C) Microsoft Corporation. All rights reserved.
//  *--------------------------------------------------------*/
// /*
//  * mockDebug.ts implements the Debug Adapter that "adapts" or translates the
//    Debug Adapter Protocol (DAP) used by the client (e.g. VS Code)
//  * into requests and events of the real "execution engine" or "debugger"
//    (here: class MockRuntime).
//  * When implementing your own debugger extension for VS Code, most of the
//    work will go into the Debug Adapter.
//  * Since the Debug Adapter is independent from VS Code, it can be used in any
//    client (IDE) supporting the Debug Adapter Protocol.
//  *
//  * The most important class of the Debug Adapter is the MockDebugSession
//    which implements many DAP requests by talking to the MockRuntime.
//  */
//  import {
//     Logger,
//     logger,
//     LoggingDebugSession,
//     InitializedEvent,
//     TerminatedEvent,
//     StoppedEvent,
//     BreakpointEvent,
//     OutputEvent,
//     ProgressStartEvent,
//     ProgressUpdateEvent,
//     ProgressEndEvent,
//     InvalidatedEvent,
//     Thread,
//     StackFrame,
//     Scope,
//     Source,
//     Handles,
//     Breakpoint,
//     MemoryEvent,
//   } from "@vscode/debugadapter";
//   import { DebugProtocol } from "@vscode/debugprotocol";
//   import { basename } from "path";
//   import {
//     MockRuntime,
//     IRuntimeBreakpoint,
//     FileAccessor,
//     RuntimeVariable,
//     timeout,
//     IRuntimeVariableType,
//   } from "./mockRuntime";
//   import { Subject } from "await-notify";
//   import * as base64 from "base64-js";

//   /**
//    * This interface describes the mock-debug specific launch attributes
//    * (which are not part of the Debug Adapter Protocol).
//    * The schema for these attributes lives in the package.json of the
//      mock-debug extension.
//    * The interface should always match this schema.
//    */
//   interface ILaunchRequestArguments extends
// DebugProtocol.LaunchRequestArguments {     /** An absolute path to the
// "program" to debug. */     program: string;
//     /** Automatically stop target after launch. If not specified, target does
// not stop. */     stopOnEntry?: boolean;
//     /** enable logging the Debug Adapter Protocol */
//     trace?: boolean;
//     /** run without debugging */
//     noDebug?: boolean;
//     /** if specified, results in a simulated compile error in launch. */
//     compileError?: "default" | "show" | "hide";
//   }

//   interface IAttachRequestArguments extends ILaunchRequestArguments {}

//   export class TypstDebugSession extends LoggingDebugSession {
//     // we don't support multiple threads, so we can use a hardcoded ID for
// the default thread     private static threadID = 1;

//     // a Mock runtime (or debugger)
//     private _runtime: MockRuntime;

//     private _variableHandles = new Handles<"locals" | "globals" |
// RuntimeVariable>();

//     private _configurationDone = new Subject();

//     private _cancellationTokens = new Map<number, boolean>();

//     private _reportProgress = false;
//     private _progressId = 10000;
//     private _cancelledProgressId: string | undefined = undefined;
//     private _isProgressCancellable = true;

//     private _valuesInHex = false;
//     private _useInvalidatedEvent = false;

//     private _addressesInHex = true;

//     /**
//      * Creates a new debug adapter that is used for one debug session.
//      * We configure the default implementation of a debug adapter here.
//      */
//     public constructor(fileAccessor: FileAccessor) {
//       super("mock-debug.txt");

//       // this debugger uses zero-based lines and columns
//       this.setDebuggerLinesStartAt1(false);
//       this.setDebuggerColumnsStartAt1(false);

//       this._runtime = new MockRuntime(fileAccessor);

//       // setup event handlers
//       this._runtime.on("stopOnEntry", () => {
//         this.sendEvent(new StoppedEvent("entry",
// TypstDebugSession.threadID));       });
//       this._runtime.on("stopOnStep", () => {
//         this.sendEvent(new StoppedEvent("step", TypstDebugSession.threadID));
//       });
//       this._runtime.on("stopOnBreakpoint", () => {
//         this.sendEvent(new StoppedEvent("breakpoint",
// TypstDebugSession.threadID));       });
//       this._runtime.on("stopOnDataBreakpoint", () => {
//         this.sendEvent(new StoppedEvent("data breakpoint",
// TypstDebugSession.threadID));       });
//       this._runtime.on("stopOnInstructionBreakpoint", () => {
//         this.sendEvent(new StoppedEvent("instruction breakpoint",
// TypstDebugSession.threadID));       });
//       this._runtime.on("stopOnException", (exception) => {
//         if (exception) {
//           this.sendEvent(new StoppedEvent(`exception(${exception})`,
// TypstDebugSession.threadID));         } else {
//           this.sendEvent(new StoppedEvent("exception",
// TypstDebugSession.threadID));         }
//       });
//       this._runtime.on("breakpointValidated", (bp: IRuntimeBreakpoint) => {
//         this.sendEvent(
//           new BreakpointEvent("changed", {
//             verified: bp.verified,
//             id: bp.id,
//           } as DebugProtocol.Breakpoint),
//         );
//       });
//       this._runtime.on("output", (type, text, filePath, line, column) => {
//         let category: string;
//         switch (type) {
//           case "prio":
//             category = "important";
//             break;
//           case "out":
//             category = "stdout";
//             break;
//           case "err":
//             category = "stderr";
//             break;
//           default:
//             category = "console";
//             break;
//         }
//         const e: DebugProtocol.OutputEvent = new OutputEvent(`${text}\n`,
// category);

//         if (text === "start" || text === "startCollapsed" || text === "end")
// {           e.body.group = text;
//           e.body.output = `group-${text}\n`;
//         }

//         e.body.source = this.createSource(filePath);
//         e.body.line = this.convertDebuggerLineToClient(line);
//         e.body.column = this.convertDebuggerColumnToClient(column);
//         this.sendEvent(e);
//       });
//       this._runtime.on("end", () => {
//         this.sendEvent(new TerminatedEvent());
//       });
//     }

//     /**
//      * Called at the end of the configuration sequence.
//      * Indicates that all breakpoints etc. have been sent to the DA and that
//        the 'launch' can start.
//      */
//     protected configurationDoneRequest(
//       response: DebugProtocol.ConfigurationDoneResponse,
//       args: DebugProtocol.ConfigurationDoneArguments,
//     ): void {
//       super.configurationDoneRequest(response, args);

//       // notify the launchRequest that configuration has finished
//       this._configurationDone.notify();
//     }

//     protected disconnectRequest(
//       response: DebugProtocol.DisconnectResponse,
//       args: DebugProtocol.DisconnectArguments,
//       request?: DebugProtocol.Request,
//     ): void {
//       console.log(
//         `disconnectRequest suspend: ${args.suspendDebuggee}, terminate:
// ${args.terminateDebuggee}`,       );
//     }

//     protected async attachRequest(
//       response: DebugProtocol.AttachResponse,
//       args: IAttachRequestArguments,
//     ) {
//       return this.launchRequest(response, args);
//     }

//     protected async launchRequest(
//       response: DebugProtocol.LaunchResponse,
//       args: ILaunchRequestArguments,
//     ) {
//       // make sure to 'Stop' the buffered logging if 'trace' is not set
//       logger.setup(args.trace ? Logger.LogLevel.Verbose :
// Logger.LogLevel.Stop, false);

//       // wait 1 second until configuration has finished (and
// configurationDoneRequest has been called)       await
// this._configurationDone.wait(1000);

//       // start the program in the runtime
//       await this._runtime.start(args.program, !!args.stopOnEntry,
// !args.noDebug);

//       if (args.compileError) {
//         // simulate a compile/build error in "launch" request:
//         // the error should not result in a modal dialog since 'showUser' is
// set to false.         // A missing 'showUser' should result in a modal
// dialog.         this.sendErrorResponse(response, {
//           id: 1001,
//           format: `compile error: some fake error.`,
//           showUser:
//             args.compileError === "show" ? true : args.compileError ===
// "hide" ? false : undefined,         });
//       } else {
//         this.sendResponse(response);
//       }
//     }

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

//     protected threadsRequest(response: DebugProtocol.ThreadsResponse): void {
//       // runtime supports no threads so just return a default thread.
//       response.body = {
//         threads: [
//           new Thread(TypstDebugSession.threadID, "thread 1"),
//           new Thread(TypstDebugSession.threadID + 1, "thread 2"),
//         ],
//       };
//       this.sendResponse(response);
//     }

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

//     private async progressSequence() {
//       const ID = "" + this._progressId++;

//       await timeout(100);

//       const title = this._isProgressCancellable ? "Cancellable operation" :
// "Long running operation";       const startEvent:
// DebugProtocol.ProgressStartEvent = new ProgressStartEvent(ID, title);
//       startEvent.body.cancellable = this._isProgressCancellable;
//       this._isProgressCancellable = !this._isProgressCancellable;
//       this.sendEvent(startEvent);
//       this.sendEvent(new OutputEvent(`start progress: ${ID}\n`));

//       let endMessage = "progress ended";

//       for (let i = 0; i < 100; i++) {
//         await timeout(500);
//         this.sendEvent(new ProgressUpdateEvent(ID, `progress: ${i}`));
//         if (this._cancelledProgressId === ID) {
//           endMessage = "progress cancelled";
//           this._cancelledProgressId = undefined;
//           this.sendEvent(new OutputEvent(`cancel progress: ${ID}\n`));
//           break;
//         }
//       }
//       this.sendEvent(new ProgressEndEvent(ID, endMessage));
//       this.sendEvent(new OutputEvent(`end progress: ${ID}\n`));

//       this._cancelledProgressId = undefined;
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

//     protected cancelRequest(
//       response: DebugProtocol.CancelResponse,
//       args: DebugProtocol.CancelArguments,
//     ) {
//       if (args.requestId) {
//         this._cancellationTokens.set(args.requestId, true);
//       }
//       if (args.progressId) {
//         this._cancelledProgressId = args.progressId;
//       }
//     }

//     protected disassembleRequest(
//       response: DebugProtocol.DisassembleResponse,
//       args: DebugProtocol.DisassembleArguments,
//     ) {
//       const memoryInt = args.memoryReference.slice(3);
//       const baseAddress = parseInt(memoryInt);
//       const offset = args.instructionOffset || 0;
//       const count = args.instructionCount;

//       const isHex = memoryInt.startsWith("0x");
//       const pad = isHex ? memoryInt.length - 2 : memoryInt.length;

//       const loc = this.createSource(this._runtime.sourceFile);

//       let lastLine = -1;

//       const instructions = this._runtime
//         .disassemble(baseAddress + offset, count)
//         .map((instruction) => {
//           let address = Math.abs(instruction.address)
//             .toString(isHex ? 16 : 10)
//             .padStart(pad, "0");
//           const sign = instruction.address < 0 ? "-" : "";
//           const instr: DebugProtocol.DisassembledInstruction = {
//             address: sign + (isHex ? `0x${address}` : `${address}`),
//             instruction: instruction.instruction,
//           };
//           // if instruction's source starts on a new line add the source to
// instruction           if (instruction.line !== undefined && lastLine !==
// instruction.line) {             lastLine = instruction.line;
//             instr.location = loc;
//             instr.line = this.convertDebuggerLineToClient(instruction.line);
//           }
//           return instr;
//         });

//       response.body = {
//         instructions: instructions,
//       };
//       this.sendResponse(response);
//     }

//     protected setInstructionBreakpointsRequest(
//       response: DebugProtocol.SetInstructionBreakpointsResponse,
//       args: DebugProtocol.SetInstructionBreakpointsArguments,
//     ) {
//       // clear all instruction breakpoints
//       this._runtime.clearInstructionBreakpoints();

//       // set instruction breakpoints
//       const breakpoints = args.breakpoints.map((ibp) => {
//         const address = parseInt(ibp.instructionReference.slice(3));
//         const offset = ibp.offset || 0;
//         return <DebugProtocol.Breakpoint>{
//           verified: this._runtime.setInstructionBreakpoint(address + offset),
//         };
//       });

//       response.body = {
//         breakpoints: breakpoints,
//       };
//       this.sendResponse(response);
//     }

//     protected customRequest(command: string, response:
// DebugProtocol.Response, args: any) {       if (command ===
// "toggleFormatting") {         this._valuesInHex = !this._valuesInHex;
//         if (this._useInvalidatedEvent) {
//           this.sendEvent(new InvalidatedEvent(["variables"]));
//         }
//         this.sendResponse(response);
//       } else {
//         super.customRequest(command, response, args);
//       }
//     }

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

mod event;
mod init;
mod request;

pub use init::*;
