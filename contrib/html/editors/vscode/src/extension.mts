/* --------------------------------------------------------------------------------------------
 * Copyright (c) Microsoft Corporation. All rights reserved.
 * Licensed under the MIT License. See License.txt in the project root for license information.
 * ------------------------------------------------------------------------------------------ */

import * as path from "path";
import * as vscode from "vscode";
import { commands, CompletionList, ExtensionContext, Uri } from "vscode";
import {
  type LanguageClientOptions,
  type ServerOptions,
  LanguageClient,
  TransportKind,
} from "vscode-languageclient/node";
import { getVirtualContent, isInsideClassAttribute, parseRawBlockRegion } from "./embeddedSupport";
import { cssActivate } from "./features/css";

let client: LanguageClient;

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

  const removeSuffix = (uri: string) => uri.replace(/\.(html|css)$/, "");
  vscode.workspace.registerTextDocumentContentProvider("embedded-content", {
    provideTextDocumentContent: (uri) => {
      const originalUri = removeSuffix(uri.path.slice(1));
      const decodedUri = decodeURIComponent(originalUri);
      console.log("provideTextDocumentContent gg", uri.path, originalUri, decodedUri);
      return virtualDocumentContents.get(decodedUri);
    },
  });

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "typst" }],
    middleware: {
      provideCompletionItem: async (document, position, context, token, next) => {
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

        const inString = res[0]?.mode === "string";
        const inRaw = res[0]?.mode === "raw";

        // If completes the content of a `class` attribute, completes classes found in the css
        // files.
        if (inString && isInsideClassAttribute(document.getText(), document.offsetAt(position))) {
          return provider.completionItems;
        }

        if (!inRaw) {
          console.log("not in raw");
          return await next(document, position, context, token);
        }

        // If not in `<style>`, do not perform request forwarding
        const virtualContent = parseRawBlockRegion(document.getText(), document.offsetAt(position));

        if (!virtualContent) {
          return await next(document, position, context, token);
        }

        const langId = virtualContent.languageId;
        if (langId !== "html" && langId !== "css") {
          return await next(document, position, context, token);
        }

        const originalUri = document.uri.toString(true);
        virtualDocumentContents.set(
          originalUri,
          getVirtualContent(document.getText(), [virtualContent], langId!),
        );

        const vdocUriString = `embedded-content://${langId}/${encodeURIComponent(originalUri)}.${langId}`;
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
