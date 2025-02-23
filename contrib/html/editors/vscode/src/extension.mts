/* --------------------------------------------------------------------------------------------
 * Copyright (c) Microsoft Corporation. All rights reserved.
 * Licensed under the MIT License. See License.txt in the project root for license information.
 * ------------------------------------------------------------------------------------------ */

import * as path from "path";
import * as vscode from "vscode";
import { commands, CompletionList, ExtensionContext, Uri } from "vscode";
import { getLanguageService } from "vscode-html-languageservice/lib/esm/htmlLanguageService";
import {
  type LanguageClientOptions,
  type ServerOptions,
  LanguageClient,
  TransportKind,
} from "vscode-languageclient/node";
import {
  getCSSVirtualContent,
  isInsideClassAttribute,
  isInsideStyleRegion,
} from "./embeddedSupport";
import { cssActivate } from "./features/css";

let client: LanguageClient;

const htmlLanguageService = getLanguageService();

export function activate(context: ExtensionContext) {
  const tinymistExtension = vscode.extensions.getExtension("myriad-dreamin.tinymist");
  if (!tinymistExtension) {
    void vscode.window.showWarningMessage(
      "Tinymist HTML:\n\nTinymist LSP feature is required. Please install Tinymist Typst Extension (myriad-dreamin.tinymist).",
    );
  }

  const provider = cssActivate(context);

  // The server is implemented in node
  const serverModule = context.asAbsolutePath(path.join("out", "server.js"));

  // If the extension is launched in debug mode then the debug server options are used
  // Otherwise the run options are used
  const serverOptions: ServerOptions = {
    run: { module: serverModule, transport: TransportKind.ipc },
    debug: {
      module: serverModule,
      transport: TransportKind.ipc,
    },
  };

  const virtualDocumentContents = new Map<string, string>();

  vscode.workspace.registerTextDocumentContentProvider("embedded-content", {
    provideTextDocumentContent: (uri) => {
      // console.log("provideTextDocumentContent gg", uri.toString(), decodeURIComponent);
      const originalUri = uri.path.slice(1).slice(0, -4);
      const decodedUri = decodeURIComponent(originalUri);
      return virtualDocumentContents.get(decodedUri);
    },
  });

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "typst" }],
    middleware: {
      provideCompletionItem: async (document, position, context, token, next) => {
        // console.log("provideCompletionItem", document.uri.toString(), position);

        const res = await vscode.commands.executeCommand<
          [{ mode: "math" | "markup" | "code" | "comment" | "string" | "raw" }]
        >("tinymist.interactCodeContext", {
          textDocument: {
            uri: document.uri.toString(),
          },
          query: [
            {
              kind: "modeAt",
              position: {
                line: position.line,
                character: position.character,
              },
            },
          ],
        });

        const inString = res[0].mode === "string";
        const inRaw = res[0].mode === "raw";

        // If in `<class>`, completes class
        if (
          inString &&
          isInsideClassAttribute(
            htmlLanguageService,
            document.getText(),
            document.offsetAt(position),
          )
        ) {
          // console.log("isInsideClassAttribute", await provider.completionItems);
          return provider.completionItems;
        }

        if (!inRaw) {
          console.log("not in raw");
          return await next(document, position, context, token);
        }

        // If not in `<style>`, do not perform request forwarding
        const virtualContent = isInsideStyleRegion(
          htmlLanguageService,
          document.getText(),
          document.offsetAt(position),
        );

        if (!virtualContent) {
          return await next(document, position, context, token);
        }

        const originalUri = document.uri.toString(true);
        virtualDocumentContents.set(
          originalUri,
          getCSSVirtualContent(htmlLanguageService, document.getText()),
        );

        const vdocUriString = `embedded-content://css/${encodeURIComponent(originalUri)}.css`;
        const vdocUri = Uri.parse(vdocUriString);
        return await commands.executeCommand<CompletionList>(
          "vscode.executeCompletionItemProvider",
          vdocUri,
          position,
          context.triggerCharacter,
        );
      },
    },
  };

  // Create the language client and start the client.
  client = new LanguageClient(
    "TinymistTypstHTMLExtension",
    "Tinymist Typst HTML Extension",
    serverOptions,
    clientOptions,
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("tinymist-html-ext.showLog", () => {
      client.outputChannel?.show();
    }),
  );

  // Start the client. This will also launch the server
  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
