import * as vscode from "vscode";
import { ExtensionContext, Uri } from "vscode";
import { LanguageClient, LanguageClientOptions } from "vscode-languageclient/browser";
import { TinymistConfig } from "./config";
import { bytesBase64Encode } from "./util";

const NAME = "Tinymist Typst Language Server";

export async function createBrowserLanguageClient(
  context: ExtensionContext,
  _config: TinymistConfig,
  clientOptions: LanguageClientOptions,
): Promise<LanguageClient> {
  const serverMain = Uri.joinPath(context.extensionUri, "out/web-server.js");
  // @ts-ignore
  const worker = new Worker(serverMain.toString(true));

  const id = Math.random().toString(36).substring(2, 15);

  // /tinymist-worker/fs/content
  const serviceWorkerScript = `
  const thisWorkerId = ${JSON.stringify(id)};
  let reqId = 0;

  const contentPromises = new Map();

  self.addEventListener("fetch", async (event) => {
    const url = new URL(event.request.url);
    console.log("Service worker fetch", url.pathname, url.searchParams.toString());

    const isFsContentRequest = url.pathname.endsWith("/tinymist-worker/fs/content");
    if (isFsContentRequest) {
      const path = url.searchParams.get("path");
      if (!path) {
        console.error("No path provided in fs content request");
        return;
      }

      console.log("Service worker fs content request for path", path);

      const contentPromise = new Promise((resolve, reject) => {
        const nextReqId = reqId;
        reqId += 1;


        contentPromises.set(nextReqId, { resolve, reject });

        self.postMessage({
          method: "tinymist/fs/content",
          params: { path, workerId: thisWorkerId, id: nextReqId },
        });
      });

      // respond with contentPromise
      event.respondWith(contentPromise);
    }
  });

  self.addEventListener("message", (event) => {
    console.log("Service worker message received:", event.data);
    const { method, params, id, workerId } = event.data;
    if (workerId !== thisWorkerId) return;
    if (method === "tinymist/fs/content/rseult") {
      const contentPromise = contentPromises.get(id);
      if (!contentPromise) {
        console.error("No content promise found for id", id);
        return;
      }
      contentPromises.delete(id);
      const response = new Response(JSON.stringify(params), {
        headers: { "Content-Type": "application/json" },
      });
      contentPromise.resolve(response);
    }
  });
  `;

  const dataUrl = `data:application/javascript;charset=utf-8,${encodeURIComponent(serviceWorkerScript)}`;

  // @ts-ignore
  navigator.serviceWorker
    .register(dataUrl)
    .then((registration: any) => {
      console.log("Service worker registered:", registration);
    })
    .catch((error: any) => {
      console.error("Service worker registration failed:", error);
    });

  // @ts-ignore
  navigator.serviceWorker.addEventListener("message", async (event) => {
    console.log("Service worker message received:", event.data);
    const workerId = event.data.workerId;
    if (workerId !== id) return;

    if (event.data.method === "tinymist/fs/content") {
      const fsUrl = vscode.Uri.file(
        // replace escaped root on windows
        event.data.params.path.replace(/[\\\/](\w+)%3A/g, (_: string, p1: string) => `${p1}:`),
      );
      const content = await vscode.workspace.fs.readFile(fsUrl).then(
        (data) => ({ content: bytesBase64Encode(data) }),
        (err) => {
          console.error("Failed to read file", event.data.params, err);
          throw err;
        },
      );

      // @ts-ignore
      navigator.serviceWorker.controller?.postMessage({
        method: "tinymist/fs/content/rseult",
        params: { workerId, id: event.data.id, params: content },
      });
    }
  });

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
    }, 60000 * 5);

    worker.addEventListener("message", onReady);
  });

  /// To this time, we start to allow vscode to receive messages from the worker.
  const client = new LanguageClient("tinymist", NAME, clientOptions, worker);
  /// Sets up the output channel for the client
  client.onNotification("tmLog", ({ data }) => clientOptions.outputChannel?.append(data));
  return client;
}
