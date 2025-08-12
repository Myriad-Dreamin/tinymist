import {
  createConnection,
  BrowserMessageReader,
  BrowserMessageWriter,
} from "vscode-languageserver/browser";

import { InitializeRequest } from "vscode-languageserver";

if ("stackTraceLimit" in Error) {
  Error.stackTraceLimit = 64;
}

// @ts-ignore
import { initSync, TinymistLanguageServer } from "tinymist-web";
// @ts-ignore
import wasmURL from "../../out/tinymist_bg.wasm";

(async function startServer() {
  const connection = createConnection(
    //@ts-ignore
    new BrowserMessageReader(self),
    //@ts-ignore
    new BrowserMessageWriter(self),
  );

  // const wasmModule = fetch(wasmURL, {
  //   headers: {
  //     "Accept-Encoding": "Accept-Encoding: gzip",
  //   },
  // }).then((wasm) => wasm.arrayBuffer());
  // initSync(await wasmModule);
  initSync(wasmURL);
  console.log(`tinymist-web ${TinymistLanguageServer.version()} wasm is loaded...`);

  let events: any[] = [];
  const bridge = new TinymistLanguageServer({
    sendEvent: (event: any): void => void events.push(event),
    sendRequest({ id, method, params }: any): void {
      connection
        .sendRequest(method, params)
        .then((result: any) => bridge.on_response({ id, result }))
        .catch((err: any) =>
          bridge.on_response({ id, error: { code: -32603, message: err.toString() } }),
        );
    },
    sendNotification: ({ method, params }: any): void =>
      void connection.sendNotification(method, params),
  });

  const h = <T>(res: T): T => {
    for (const event of events.splice(0)) {
      bridge.on_event(event);
    }
    return res;
  };

  connection.onInitialize((params) => h(bridge.on_request(InitializeRequest.method, params)));
  connection.onRequest((m, p) => h(bridge.on_request(m, p)));
  connection.onNotification((m, p) => h(bridge.on_notification(m, p)));

  connection.sendNotification("serverWorkerReady");
  // Starts the language server
  connection.listen();
})();
