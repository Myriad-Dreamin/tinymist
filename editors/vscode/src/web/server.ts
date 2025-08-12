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
      console.log("do sendRequest", id, method, params);
      connection
        .sendRequest(method, params)
        .then((result: any) => bridge.on_response({ id, result }))
        .catch((err: any) =>
          bridge.on_response({ id, error: { code: -32603, message: err.toString() } }),
        );
    },
    fsContent(path: string): any {
      console.log("do fsContent", path);
      // return connection.sendRequest("tinymist/fs/content", { path });

      // use xmlHttpRequest to get the file content synchronously
      // @ts-ignore
      const xhr = new XMLHttpRequest();
      // /tinymist-worker/

      xhr.open("GET", `/tinymist-worker/fs/content?path=${encodeURIComponent(path)}`, false);
      xhr.setRequestHeader("Accept", "application/json");
      xhr.send();

      if (xhr.status !== 200) {
        const msg = `Failed to get file content: ${xhr.status} ${xhr.statusText}`;
        throw new Error(msg);
      }
      const content = JSON.parse(xhr.responseText);

      return content;
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
