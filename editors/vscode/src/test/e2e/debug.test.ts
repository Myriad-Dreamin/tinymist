/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

// import assert = require("assert");
// import * as Path from "path";
// import { DebugClient } from "@vscode/debugadapter-testsupport";
// import { DebugProtocol } from "@vscode/debugprotocol";

// suite("Node Debug Adapter", () => {
//   const DEBUG_ADAPTER = "./out/debugAdapter.js";

//   const PROJECT_ROOT = Path.join(__dirname, "../../");
//   const DATA_ROOT = Path.join(PROJECT_ROOT, "src/tests/data/");

//   let dc: DebugClient;

//   setup(() => {
//     dc = new DebugClient("node", DEBUG_ADAPTER, "mock");
//     return dc.start();
//   });

//   teardown(() => dc.stop());

//   suite("basic", () => {
//     test("unknown request should produce error", (done) => {
//       dc.send("illegal_request")
//         .then(() => {
//           done(new Error("does not report error on unknown request"));
//         })
//         .catch(() => {
//           done();
//         });
//     });
//   });

//   suite("initialize", () => {
//     test("should return supported features", () => {
//       return dc.initializeRequest().then((response) => {
//         response.body = response.body || {};
//         assert.equal(response.body.supportsConfigurationDoneRequest, true);
//       });
//     });

//     test("should produce error for invalid 'pathFormat'", (done) => {
//       dc.initializeRequest({
//         adapterID: "mock",
//         linesStartAt1: true,
//         columnsStartAt1: true,
//         pathFormat: "url",
//       })
//         .then((response) => {
//           done(new Error("does not report error on invalid 'pathFormat' attribute"));
//         })
//         .catch((err) => {
//           // error expected
//           done();
//         });
//     });
//   });

//   suite("launch", () => {
//     test("should run program to the end", () => {
//       const PROGRAM = Path.join(DATA_ROOT, "test.md");

//       return Promise.all([
//         dc.configurationSequence(),
//         dc.launch({ program: PROGRAM }),
//         dc.waitForEvent("terminated"),
//       ]);
//     });

//     test("should stop on entry", () => {
//       const PROGRAM = Path.join(DATA_ROOT, "test.md");
//       const ENTRY_LINE = 1;

//       return Promise.all([
//         dc.configurationSequence(),
//         dc.launch({ program: PROGRAM, stopOnEntry: true }),
//         dc.assertStoppedLocation("entry", { line: ENTRY_LINE }),
//       ]);
//     });
//   });

//   suite("setBreakpoints", () => {
//     test("should stop on a breakpoint", () => {
//       const PROGRAM = Path.join(DATA_ROOT, "test.md");
//       const BREAKPOINT_LINE = 2;

//       return dc.hitBreakpoint({ program: PROGRAM }, { path: PROGRAM, line: BREAKPOINT_LINE });
//     });

//     test("hitting a lazy breakpoint should send a breakpoint event", () => {
//       const PROGRAM = Path.join(DATA_ROOT, "testLazyBreakpoint.md");
//       const BREAKPOINT_LINE = 3;

//       return Promise.all([
//         dc.hitBreakpoint(
//           { program: PROGRAM },
//           { path: PROGRAM, line: BREAKPOINT_LINE, verified: false },
//         ),

//         dc.waitForEvent("breakpoint").then((event) => {
//           const bpevent = event as DebugProtocol.BreakpointEvent;
//           assert.strictEqual(bpevent.body.breakpoint.verified, true, "event mismatch: verified");
//         }),
//       ]);
//     });
//   });

//   suite("setExceptionBreakpoints", () => {
//     test("should stop on an exception", () => {
//       const PROGRAM_WITH_EXCEPTION = Path.join(DATA_ROOT, "testWithException.md");
//       const EXCEPTION_LINE = 4;

//       return Promise.all([
//         dc
//           .waitForEvent("initialized")
//           .then((event) => {
//             return dc.setExceptionBreakpointsRequest({
//               filters: ["otherExceptions"],
//             });
//           })
//           .then((response) => {
//             return dc.configurationDoneRequest();
//           }),

//         dc.launch({ program: PROGRAM_WITH_EXCEPTION }),

//         dc.assertStoppedLocation("exception", { line: EXCEPTION_LINE }),
//       ]);
//     });
//   });
// });
