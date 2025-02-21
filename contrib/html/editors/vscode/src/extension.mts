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
import { getCSSVirtualContent, isInsideStyleRegion } from "./embeddedSupport";

let client: LanguageClient;

const htmlLanguageService = getLanguageService();

export function activate(context: ExtensionContext) {
  console.log("start!!!!!!!!!!");

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
      console.log("provideTextDocumentContent gg", uri.toString(), decodeURIComponent);
      const originalUri = uri.path.slice(1).slice(0, -4);
      const decodedUri = decodeURIComponent(originalUri);
      return virtualDocumentContents.get(decodedUri);
    },
  });

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "typst" }],
    middleware: {
      provideCompletionItem: async (document, position, context, token, next) => {
        console.log("provideCompletionItem", document.uri.toString(), position);

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
  console.log("deactivate!!!!!!!!!!");
  if (!client) {
    console.log("client!!!!!!!!!!");
    return undefined;
  }
  return client.stop();
}
