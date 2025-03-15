"use strict";

import * as vscode from "vscode";
import { WorkspaceFolder, DebugConfiguration, ProviderResult, CancellationToken } from "vscode";

export const TYPST_DEBUGGER_TYPE = "myriaddreamin.typst-debugger";

export function debugActivate(context: vscode.ExtensionContext) {
  const factory: vscode.DebugAdapterDescriptorFactory = new DebugAdapterExecutableFactory();

  context.subscriptions.push(
    vscode.commands.registerCommand("tinymist.runEditorContents", (resource: vscode.Uri) => {
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
    }),
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
          stopOnEntry: true,
        });
      }
    }),
    // vscode.commands.registerCommand("extension.mock-debug.toggleFormatting", (variable) => {
    //   const ds = vscode.debug.activeDebugSession;
    //   if (ds) {
    //     ds.customRequest("toggleFormatting");
    //   }
    // }),
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
  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider(
      TYPST_DEBUGGER_TYPE,
      {
        provideDebugConfigurations(): ProviderResult<DebugConfiguration[]> {
          return [
            {
              name: "Dynamic Launch",
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
  // The following use of a DebugAdapter factory shows how to control what debug adapter executable is used.
  // Since the code implements the default behavior, it is absolutely not neccessary and we show it here only for educational purpose.
  createDebugAdapterDescriptor(
    session: vscode.DebugSession,
    executable: vscode.DebugAdapterExecutable | undefined,
  ): ProviderResult<vscode.DebugAdapterDescriptor> {
    const isProdMode = true; // context.extensionMode === ExtensionMode.Production;

    // param "executable" contains the executable optionally specified in the package.json (if any)
    console.log("executable from configuration", executable);

    /// Set the `RUST_BACKTRACE` environment variable to `full` to print full backtrace on error. This is useless in
    /// production mode because we don't put the debug information in the binary.
    ///
    /// Note: Developers can still download the debug information from the GitHub Releases and enable the backtrace
    /// manually by themselves.
    const RUST_BACKTRACE = isProdMode ? "1" : "full";

    // use the executable specified in the package.json if it exists or determine it based on some other information (e.g. the session)
    if (!executable) {
      const command = "absolute path to my DA executable";
      const args = ["dap"];
      const options = {
        cwd: "working directory for executable",
        env: { RUST_BACKTRACE },
      };
      executable = new vscode.DebugAdapterExecutable(command, args, options);
    }

    const command = executable.command;
    const rootCwd = executable.options?.cwd || session.workspaceFolder?.uri.fsPath;
    const args = ["dap"];

    const options = executable.options || {};
    options.cwd = rootCwd;
    options.env = Object.assign({}, process.env, {
      ...(options.env || {}),
      RUST_BACKTRACE,
    }) as Record<string, string>;

    executable = new vscode.DebugAdapterExecutable(command, args, options);

    // make VS Code launch the DA executable
    return executable;
  }
}
