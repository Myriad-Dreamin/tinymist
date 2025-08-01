import { ExtensionContext, Uri } from "vscode";
import { LanguageClient, LanguageClientOptions } from "vscode-languageclient/browser";

export async function createBrowserLanguageClient(
  context: ExtensionContext,
  clientOptions: LanguageClientOptions,
): Promise<LanguageClient> {
  const serverMain = Uri.joinPath(context.extensionUri, "out/web-server.js");

  // @ts-ignore
  const worker = new Worker(serverMain.toString(true));

  let serverWorkerReady: ((value: unknown) => void) | null = null;
  let workerTimeout: ReturnType<typeof setTimeout> | null = null;
  const serverWorkerPromise = new Promise((resolve, reject) => {
    serverWorkerReady = resolve;
    workerTimeout = setTimeout(() => {
      reject(new Error("worker timeout"));
    }, 10000);
  });

  worker.addEventListener("message", function onServerWorkerReady(e: MessageEvent) {
    if (e.data.method !== "serverWorkerReady") return;
    worker.removeEventListener("message", onServerWorkerReady);
    serverWorkerReady!(true);
    clearTimeout(workerTimeout!);
  });

  await serverWorkerPromise;
  const client = new LanguageClient(
    "tinymist",
    "Tinymist Typst Language Server",
    clientOptions,
    worker,
  );

  return client;
}
