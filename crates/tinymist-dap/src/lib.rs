//! Fast debugger implementation for typst.

//       this._runtime = new MockRuntime(fileAccessor);

//       this._runtime.on("breakpointValidated", (bp: IRuntimeBreakpoint) => {
//         this.sendEvent(
//           new BreakpointEvent("changed", {
//             verified: bp.verified,
//             id: bp.id,
//           } as DebugProtocol.Breakpoint),
//         );
//       });
//       this._runtime.on("end", () => {
//         this.sendEvent(new TerminatedEvent());
//       });
