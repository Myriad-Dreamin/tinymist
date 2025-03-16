"use strict";

import * as vscode from "vscode";
import { WorkspaceFolder, DebugConfiguration, ProviderResult, CancellationToken } from "vscode";
import { IContext } from "./context";
// import { ProtocolServer } from "@vscode/debugadapter/lib/protocol";
// import { spawn } from "cross-spawn";

export const TYPST_DEBUGGER_TYPE = "myriaddreamin.typst-debugger";

export function debugActivate(context: IContext) {
  const factory: vscode.DebugAdapterDescriptorFactory = new DebugAdapterExecutableFactory(context);

  context.subscriptions.push(
    // ,
    //   {
    //     "command": "tinymist.runEditorContents",
    //     "title": "Run File",
    //     "category": "Typst Debug",
    //     "enablement": "!inDebugMode",
    //     "icon": "$(play)"
    //   }
    // vscode.commands.registerCommand("tinymist.runEditorContents", (resource: vscode.Uri) => {
    //   let targetResource = resource;
    //   if (!targetResource && vscode.window.activeTextEditor) {
    //     targetResource = vscode.window.activeTextEditor.document.uri;
    //   }
    //   if (targetResource) {
    //     vscode.debug.startDebugging(
    //       undefined,
    //       {
    //         type: TYPST_DEBUGGER_TYPE,
    //         name: "Run File",
    //         request: "launch",
    //         program: targetResource.fsPath,
    //         root: vscode.workspace.getWorkspaceFolder(targetResource)?.uri.fsPath,
    //       },
    //       { noDebug: true },
    //     );
    //   }
    // }),
    vscode.commands.registerCommand("tinymist.debugEditorContents", (resource: vscode.Uri) => {
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
          root: vscode.workspace.getWorkspaceFolder(targetResource)?.uri.fsPath,
          stopOnEntry: true,
        });
      }
    }),
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
        value: "main.typ",
      });
    }),
  );

  // register a configuration provider for 'mock' debug type
  const provider = new MockConfigurationProvider();
  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider(TYPST_DEBUGGER_TYPE, provider),
  );

  // register a dynamic configuration provider for 'mock' debug type
  // context.subscriptions.push(
  //   vscode.debug.registerDebugConfigurationProvider(
  //     TYPST_DEBUGGER_TYPE,
  //     {
  //       provideDebugConfigurations(): ProviderResult<DebugConfiguration[]> {
  //         return [
  //           {
  //             name: "Dynamic Launch",
  //             request: "launch",
  //             type: TYPST_DEBUGGER_TYPE,
  //             program: "${file}",
  //           },
  //         ];
  //       },
  //     },
  //     vscode.DebugConfigurationProviderTriggerKind.Dynamic,
  //   ),
  // );

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
  // context.subscriptions.push(
  //   vscode.languages.registerInlineValuesProvider("typst", {
  //     provideInlineValues(
  //       document: vscode.TextDocument,
  //       viewport: vscode.Range,
  //       context: vscode.InlineValueContext,
  //     ): vscode.ProviderResult<vscode.InlineValue[]> {
  //       const allValues: vscode.InlineValue[] = [];

  //       for (let l = viewport.start.line; l <= context.stoppedLocation.end.line; l++) {
  //         const line = document.lineAt(l);
  //         var regExp = /\$([a-z][a-z0-9]*)/gi; // variables are words starting with '$'
  //         do {
  //           var m = regExp.exec(line.text);
  //           if (m) {
  //             const varName = m[1];
  //             const varRange = new vscode.Range(l, m.index, l, m.index + varName.length);

  //             // some literal text
  //             //allValues.push(new vscode.InlineValueText(varRange, `${varName}: ${viewport.start.line}`));

  //             // value found via variable lookup
  //             allValues.push(new vscode.InlineValueVariableLookup(varRange, varName, false));

  //             // value determined via expression evaluation
  //             //allValues.push(new vscode.InlineValueEvaluatableExpression(varRange, varName));
  //           }
  //         } while (m);
  //       }

  //       return allValues;
  //     },
  //   }),
  // );
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

class DebugAdapterExecutableFactory implements vscode.DebugAdapterDescriptorFactory {
  // static outputChannel = vscode.window.createOutputChannel("Tinymist Debugging", "log");

  constructor(private readonly context: IContext) {}

  // The following use of a DebugAdapter factory shows how to control what debug adapter executable is used.
  // Since the code implements the default behavior, it is absolutely not necessary and we show it here only for educational purpose.
  createDebugAdapterDescriptor(
    session: vscode.DebugSession,
    executable: vscode.DebugAdapterExecutable | undefined,
  ): ProviderResult<vscode.DebugAdapterDescriptor> {
    const isProdMode = this.context.context.extensionMode === vscode.ExtensionMode.Production;

    const hasMirrorFlag = () => {
      return executable?.args?.some((arg) => arg.startsWith("--mirror=") || arg === "--mirror");
    };

    /// The `--mirror` flag is only used in development/test mode for testing
    const mirrorFlag = isProdMode ? [] : hasMirrorFlag() ? [] : ["--mirror", "tinymist-dap.log"];
    /// Set the `RUST_BACKTRACE` environment variable to `full` to print full backtrace on error. This is useless in
    /// production mode because we don't put the debug information in the binary.
    ///
    /// Note: Developers can still download the debug information from the GitHub Releases and enable the backtrace
    /// manually by themselves.
    const RUST_BACKTRACE = isProdMode ? "1" : "full";

    const args = executable?.args?.length
      ? [...executable.args, ...mirrorFlag]
      : ["dap", ...mirrorFlag];

    const command = executable?.command || this.context.tinymistExecutable;

    console.log("dap executable", executable, "=>", command, args);

    if (!command) {
      vscode.window.showErrorMessage("Cannot find tinymist executable to debug");
      return;
    }

    // todo: resolve the cwd according to the program being debugged
    const cwd =
      executable?.options?.cwd ||
      session.workspaceFolder?.uri.fsPath ||
      (vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders.length > 0
        ? vscode.workspace.workspaceFolders[0].uri.fsPath
        : undefined);

    // const child = spawn(command, args, {
    //   stdio: "pipe",
    //   cwd,
    //   env: Object.assign({}, process.env, {
    //     ...(executable?.options?.env || {}),
    //     RUST_BACKTRACE,
    //   }),
    // });

    // const server = new ProtocolServer();
    // server.start(child.stdout, child.stdin);
    // child.stderr.on("data", (data) => {
    //   DebugAdapterExecutableFactory.outputChannel.append(data.toString());
    // });
    // child.on("exit", (code) => {
    //   DebugAdapterExecutableFactory.outputChannel.appendLine(
    //     `Tinymist Debug Adapter exited with code ${code}`,
    //   );
    // });

    // return new vscode.DebugAdapterInlineImplementation(server);

    return new vscode.DebugAdapterExecutable(command, args, {
      cwd,
      env: {
        ...(executable?.options?.env || {}),
        RUST_BACKTRACE,
      },
    });
  }
}
