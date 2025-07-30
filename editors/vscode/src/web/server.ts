import {
  createConnection,
  BrowserMessageReader,
  BrowserMessageWriter,
} from "vscode-languageserver/browser";

import { InitializeRequest } from "vscode-languageserver";
import type { Connection } from "vscode-languageserver";

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

  let events: any[] = [];
  const send_event = (event: any) => {
    events.push(event);
  };

  // const wasmModule = fetch(wasmURL, {
  //   headers: {
  //     "Accept-Encoding": "Accept-Encoding: gzip",
  //   },
  // }).then((wasm) => wasm.arrayBuffer());
  // initSync(await wasmModule);
  console.log("Initializing Tinymist WebAssembly module...");
  initSync(wasmURL);
  console.log("Initialized Tinymist WebAssembly module...");

  const bridge = new TinymistLanguageServer(
    send_event,
    connection.sendNotification,
    connection.sendRequest,
  );
  connection.sendNotification("serverWorkerReady");
  // todo: output channel

  connection.onInitialize((params) => {
    const res = bridge.on_request(InitializeRequest.method, params);
    console.log("[tinymist] onInitialize response:", res);
    for (const event of events.splice(0)) {
      bridge.on_server_event(event);
    }
    return res;
  });
  connection.onRequest((m, p) => {
    console.log("[tinymist] onRequest:", m, p);
    const res = bridge.on_request(m, p);
    console.log("[tinymist] onRequest response:", res);
    for (const event of events.splice(0)) {
      bridge.on_server_event(event);
    }
    return res;
  });
  connection.onNotification((m, p) => {
    console.log("[tinymist] onNotification:", m, p);
    bridge.on_notification(m, p);
    // No response needed for notifications
    console.log("[tinymist] onNotification processed");
    for (const event of events.splice(0)) {
      bridge.on_server_event(event);
    }
  });

  // Starts the language server
  connection.listen();

  console.log("Language server worker running...");
})();
