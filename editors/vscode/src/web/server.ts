import {
  createConnection,
  BrowserMessageReader,
  BrowserMessageWriter,
} from "vscode-languageserver/browser";
import {
  DidOpenTextDocumentParams,
  DidCloseTextDocumentNotification,
  DidOpenTextDocumentNotification,
  InitializeRequest,
} from "vscode-languageserver";
import type { Connection } from "vscode-languageserver";

// @ts-ignore
import { initSync, TinymistLanguageServer } from "tinymist-web";

declare const __EXTENSION_URL__: string;

(async function startServer() {
  const wasmURL = new URL("out/tinymist_core_bg.wasm", __EXTENSION_URL__);

  const VALID_PROTOCOLS = ["file", "vscode-vfs", "vscode-test-web"];

  const wasmModule = fetch(wasmURL, {
    headers: {
      "Accept-Encoding": "Accept-Encoding: gzip",
    },
  }).then((wasm) => wasm.arrayBuffer());

  const connection = createConnection(
    //@ts-ignore
    new BrowserMessageReader(self),
    //@ts-ignore
    new BrowserMessageWriter(self),
  );

  initSync(await wasmModule);

  const bridge = new TinymistLanguageServer(
    connection.sendDiagnostics,
    connection.sendNotification,
    connection.sendRequest,
  );

  initConnection(connection, bridge);
  connection.sendNotification("serverWorkerReady");

  function initConnection(connection: Connection, bridge: TinymistLanguageServer) {
    let initializationOptions: { [key: string]: any } = {};
    connection.onInitialize((params) => {
      try {
        initializationOptions = JSON.parse(params.initializationOptions);
        params.initializationOptions = initializationOptions;
      } catch (err) {
        console.error("Invalid initialization options");
        throw err;
      }

      return bridge.on_request(InitializeRequest.method, params);
    });

    const notifications: [string, unknown][] = [];
    async function consumeNotification() {
      const notification = notifications[notifications.length - 1];
      if (!notifications) return;
      try {
        await bridge.on_notification(...notification);
      } catch (err) {
        console.warn(err);
      }
      notifications.pop();
      if (notifications.length > 0) consumeNotification();
    }

    connection.onNotification((method: string, params: unknown) => {
      // vscode.dev sends didOpen notification twice
      // including a notification with a read only github:// url
      // instead of vscode-vfs://
      if (
        method === DidOpenTextDocumentNotification.method ||
        method === DidCloseTextDocumentNotification.method
      ) {
        const [protocol] = (params as DidOpenTextDocumentParams).textDocument.uri.split("://");
        if (!VALID_PROTOCOLS.includes(protocol)) return;
      }

      notifications.push([method, params]);
      if (notifications.length === 1) consumeNotification();
    });

    connection.onRequest((method: string, params: unknown) => {
      if (notifications.length > 0) return null;
      return bridge.on_request(method, params);
    });

    connection.listen();
  }
})();
