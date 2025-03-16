//! Fast debugger implementation for typst.

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
