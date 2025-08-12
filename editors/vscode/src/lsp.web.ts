import * as vscode from "vscode";
import { ExtensionContext, Uri } from "vscode";
import { LanguageClient, LanguageClientOptions } from "vscode-languageclient/browser";
import { TinymistConfig } from "./config";

const NAME = "Tinymist Typst Language Server";

export async function createBrowserLanguageClient(
  context: ExtensionContext,
  _config: TinymistConfig,
  clientOptions: LanguageClientOptions,
): Promise<LanguageClient> {
  const serverMain = Uri.joinPath(context.extensionUri, "out/web-server.js");
  // @ts-ignore
  const worker = new Worker(serverMain.toString(true));

  /// Waits for the server worker to be ready before returning the client
  await new Promise((resolve, reject) => {
    function onReady(e: MessageEvent) {
      if (e.data.method !== "serverWorkerReady") return;
      worker.removeEventListener("message", onReady);
      resolve!(true);
      clearTimeout(workerTimeout!);
    }

    const workerTimeout = setTimeout(() => {
      worker.removeEventListener("message", onReady);
      reject(new Error("failed to initialize server worker: timeout"));
    }, 10000);

    worker.addEventListener("message", onReady);
  });

  /// To this time, we start to allow vscode to receive messages from the worker.
  const client = new LanguageClient("tinymist", NAME, clientOptions, worker);
  /// Sets up the output channel for the client
  client.onNotification("tmLog", ({ data }) => clientOptions.outputChannel?.append(data));
  return client;
}
