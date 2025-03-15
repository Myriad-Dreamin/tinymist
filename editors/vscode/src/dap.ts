/*---------------------------------------------------------
 * Copyright (C) Microsoft Corporation. All rights reserved.
 *--------------------------------------------------------*/

/*---------------------------------------------------------
 * Copyright (C) Microsoft Corporation. All rights reserved.
 *--------------------------------------------------------*/
/*
 * activateMockDebug.ts containes the shared extension code that can be executed both in node.js and the browser.
 */

"use strict";

import * as vscode from "vscode";
import { WorkspaceFolder, DebugConfiguration, ProviderResult, CancellationToken } from "vscode";
import { TypstDebugSession } from "./dap/mockDebug";
import { FileAccessor } from "./dap/mockRuntime";

export const TYPST_DEBUGGER_TYPE = "myriaddreamin.typst-debugger";

// import { promises as fs } from "fs";
// import * as Net from "net";
// import { FileAccessor } from "./mockRuntime";

// /*
//  * debugAdapter.js is the entrypoint of the debug adapter when it runs as a separate process.
//  */

// /*
//  * Since here we run the debug adapter as a separate ("external") process, it has no access to VS Code API.
//  * So we can only use node.js API for accessing files.
//  */
// const fsAccessor: FileAccessor = {
//   isWindows: process.platform === "win32",
//   readFile(path: string): Promise<Uint8Array> {
//     return fs.readFile(path);
//   },
//   writeFile(path: string, contents: Uint8Array): Promise<void> {
//     return fs.writeFile(path, contents);
//   },
// };

// /*
//  * When the debug adapter is run as an external process,
//  * normally the helper function DebugSession.run(...) takes care of everything:
//  *
//  * 	MockDebugSession.run(MockDebugSession);
//  *
//  * but here the helper is not flexible enough to deal with a debug session constructors with a parameter.
//  * So for now we copied and modified the helper:
//  */

// // first parse command line arguments to see whether the debug adapter should run as a server
// let port = 0;
// const args = process.argv.slice(2);
// args.forEach(function (val, index, array) {
//   const portMatch = /^--server=(\d{4,5})$/.exec(val);
//   if (portMatch) {
//     port = parseInt(portMatch[1], 10);
//   }
// });

// if (port > 0) {
//   // start a server that creates a new session for every connection request
//   console.error(`waiting for debug protocol on port ${port}`);
//   Net.createServer((socket) => {
//     console.error(">> accepted connection from client");
//     socket.on("end", () => {
//       console.error(">> client connection closed\n");
//     });
//     const session = new TypstDebugSession(fsAccessor);
//     session.setRunAsServer(true);
//     session.start(socket, socket);
//   }).listen(port);
// } else {
//   // start a single session that communicates via stdin/stdout
//   const session = new TypstDebugSession(fsAccessor);
//   process.on("SIGTERM", () => {
//     session.shutdown();
//   });
//   session.start(process.stdin, process.stdout);
// }

export function activateMockDebug(
  context: vscode.ExtensionContext,
  factory?: vscode.DebugAdapterDescriptorFactory,
) {
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "extension.mock-debug.runEditorContents",
      (resource: vscode.Uri) => {
        let targetResource = resource;
        if (!targetResource && vscode.window.activeTextEditor) {
          targetResource = vscode.window.activeTextEditor.document.uri;
        }
        if (targetResource) {
          vscode.debug.startDebugging(
            undefined,
            {
              type: TYPST_DEBUGGER_TYPE,
              name: "Run File",
              request: "launch",
              program: targetResource.fsPath,
            },
            { noDebug: true },
          );
        }
      },
    ),
    vscode.commands.registerCommand(
      "extension.mock-debug.debugEditorContents",
      (resource: vscode.Uri) => {
        let targetResource = resource;
        if (!targetResource && vscode.window.activeTextEditor) {
          targetResource = vscode.window.activeTextEditor.document.uri;
        }
        if (targetResource) {
          vscode.debug.startDebugging(undefined, {
            type: TYPST_DEBUGGER_TYPE,
            name: "Debug File",
            request: "launch",
            program: targetResource.fsPath,
            stopOnEntry: true,
          });
        }
      },
    ),
    vscode.commands.registerCommand("extension.mock-debug.toggleFormatting", (variable) => {
      const ds = vscode.debug.activeDebugSession;
      if (ds) {
        ds.customRequest("toggleFormatting");
      }
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("myriaddreamin.typst-debugger.getProgramName", (config) => {
      return vscode.window.showInputBox({
        placeHolder: "Please enter the name of a typst file in the workspace folder",
        value: "test.typ",
      });
    }),
  );

  // register a configuration provider for 'mock' debug type
  const provider = new MockConfigurationProvider();
  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider("myriaddeamin.typst-debugger", provider),
  );

  // register a dynamic configuration provider for 'mock' debug type
  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider(
      TYPST_DEBUGGER_TYPE,
      {
        provideDebugConfigurations(
          folder: WorkspaceFolder | undefined,
        ): ProviderResult<DebugConfiguration[]> {
          return [
            {
              name: "Dynamic Launch",
              request: "launch",
              type: TYPST_DEBUGGER_TYPE,
              program: "${file}",
            },
            {
              name: "Another Dynamic Launch",
              request: "launch",
              type: TYPST_DEBUGGER_TYPE,
              program: "${file}",
            },
            {
              name: "Mock Launch",
              request: "launch",
              type: TYPST_DEBUGGER_TYPE,
              program: "${file}",
            },
          ];
        },
      },
      vscode.DebugConfigurationProviderTriggerKind.Dynamic,
    ),
  );

  if (!factory) {
    factory = new InlineDebugAdapterFactory();
  }
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory(TYPST_DEBUGGER_TYPE, factory),
  );
  if ("dispose" in factory) {
    context.subscriptions.push(factory as any);
  }

  // override VS Code's default implementation of the debug hover
  // here we match only Mock "variables", that are words starting with an '$'
  context.subscriptions.push(
    vscode.languages.registerEvaluatableExpressionProvider("typst", {
      provideEvaluatableExpression(
        document: vscode.TextDocument,
        position: vscode.Position,
      ): vscode.ProviderResult<vscode.EvaluatableExpression> {
        const VARIABLE_REGEXP = /\$[a-z][a-z0-9]*/gi;
        const line = document.lineAt(position.line).text;

        let m: RegExpExecArray | null;
        while ((m = VARIABLE_REGEXP.exec(line))) {
          const varRange = new vscode.Range(
            position.line,
            m.index,
            position.line,
            m.index + m[0].length,
          );

          if (varRange.contains(position)) {
            return new vscode.EvaluatableExpression(varRange);
          }
        }
        return undefined;
      },
    }),
  );

  // override VS Code's default implementation of the "inline values" feature"
  context.subscriptions.push(
    vscode.languages.registerInlineValuesProvider("typst", {
      provideInlineValues(
        document: vscode.TextDocument,
        viewport: vscode.Range,
        context: vscode.InlineValueContext,
      ): vscode.ProviderResult<vscode.InlineValue[]> {
        const allValues: vscode.InlineValue[] = [];

        for (let l = viewport.start.line; l <= context.stoppedLocation.end.line; l++) {
          const line = document.lineAt(l);
          var regExp = /\$([a-z][a-z0-9]*)/gi; // variables are words starting with '$'
          do {
            var m = regExp.exec(line.text);
            if (m) {
              const varName = m[1];
              const varRange = new vscode.Range(l, m.index, l, m.index + varName.length);

              // some literal text
              //allValues.push(new vscode.InlineValueText(varRange, `${varName}: ${viewport.start.line}`));

              // value found via variable lookup
              allValues.push(new vscode.InlineValueVariableLookup(varRange, varName, false));

              // value determined via expression evaluation
              //allValues.push(new vscode.InlineValueEvaluatableExpression(varRange, varName));
            }
          } while (m);
        }

        return allValues;
      },
    }),
  );
}

class MockConfigurationProvider implements vscode.DebugConfigurationProvider {
  /**
   * Massage a debug configuration just before a debug session is being launched,
   * e.g. add all missing attributes to the debug configuration.
   */
  resolveDebugConfiguration(
    folder: WorkspaceFolder | undefined,
    config: DebugConfiguration,
    token?: CancellationToken,
  ): ProviderResult<DebugConfiguration> {
    // if launch.json is missing or empty
    if (!config.type && !config.request && !config.name) {
      const editor = vscode.window.activeTextEditor;
      if (editor && editor.document.languageId === "typst") {
        config.type = TYPST_DEBUGGER_TYPE;
        config.name = "Launch";
        config.request = "launch";
        config.program = "${file}";
        config.stopOnEntry = true;
      }
    }

    if (!config.program) {
      return vscode.window.showInformationMessage("Cannot find a program to debug").then((_) => {
        return undefined; // abort launch
      });
    }

    return config;
  }
}

export const workspaceFileAccessor: FileAccessor = {
  isWindows: typeof process !== "undefined" && process.platform === "win32",
  async readFile(path: string): Promise<Uint8Array> {
    let uri: vscode.Uri;
    try {
      uri = pathToUri(path);
    } catch (e) {
      return new TextEncoder().encode(`cannot read '${path}'`);
    }

    return await vscode.workspace.fs.readFile(uri);
  },
  async writeFile(path: string, contents: Uint8Array) {
    await vscode.workspace.fs.writeFile(pathToUri(path), contents);
  },
};

function pathToUri(path: string) {
  try {
    return vscode.Uri.file(path);
  } catch (e) {
    return vscode.Uri.parse(path);
  }
}

class InlineDebugAdapterFactory implements vscode.DebugAdapterDescriptorFactory {
  createDebugAdapterDescriptor(
    _session: vscode.DebugSession,
  ): ProviderResult<vscode.DebugAdapterDescriptor> {
    return new vscode.DebugAdapterInlineImplementation(
      new TypstDebugSession(workspaceFileAccessor),
    );
  }
}
