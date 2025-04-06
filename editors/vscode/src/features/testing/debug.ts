import * as vscode from "vscode";
import { WorkspaceFolder, DebugConfiguration, ProviderResult, CancellationToken } from "vscode";
import { IContext } from "../../context";
import { DebugAdapterExecutableFactory } from "../../dap";
import { isTypstDocument } from "../../util";

export const TYPST_DEBUGGER_TYPE = "myriaddreamin.typst-debugger";

export function testingDebugActivate(context: IContext) {
  const factory: vscode.DebugAdapterDescriptorFactory = new DebugAdapterExecutableFactory(context);

  context.subscriptions.push(
    vscode.commands.registerCommand("tinymist.debugCurrentFile", (resource: vscode.Uri) => {
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
    vscode.commands.registerCommand("tinymist.getcursorStyleChains", () => {
      vscode.debug.activeDebugSession?.customRequest("getcursorStyleChains");
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("tinymist.debuggerGetProgramName", (config) => {
      return vscode.window.showInputBox({
        placeHolder: "Please enter the path to a typst file in the workspace folder",
        value: findDefaultFileForDebugging(),
      });

      function findDefaultFileForDebugging() {
        const editor = IContext.currentActiveEditor();
        const uri = editor?.document?.uri;
        if (uri) {
          const workspaceFolder = vscode.workspace.getWorkspaceFolder(uri);
          if (workspaceFolder) {
            return vscode.workspace.asRelativePath(uri, false);
          }
        }

        return "main.typ";
      }
    }),
  );

  // register a configuration provider
  const provider = new TypstConfigurationProvider();
  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider(TYPST_DEBUGGER_TYPE, provider),
  );

  // register a dynamic configuration provider
  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider(
      TYPST_DEBUGGER_TYPE,
      {
        provideDebugConfigurations(): ProviderResult<DebugConfiguration[]> {
          return [
            {
              name: "Launch Typst File",
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
}

class TypstConfigurationProvider implements vscode.DebugConfigurationProvider {
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
      if (isTypstDocument(editor?.document)) {
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
