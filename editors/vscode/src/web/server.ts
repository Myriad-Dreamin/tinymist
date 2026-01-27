import {
  createConnection,
  BrowserMessageReader,
  BrowserMessageWriter,
} from "vscode-languageserver/browser";

import { InitializeRequest } from "vscode-languageserver";

// @ts-ignore
import { initSync, TinymistLanguageServer } from "tinymist-web";
// @ts-ignore
import wasmURL from "../../out/tinymist_bg.wasm";

/**
 * The rust stack trace is deep and so we have to increase the limit to check full content.
 */
if ("stackTraceLimit" in Error) {
  Error.stackTraceLimit = 64;
}

(async function startServer() {
  const connection = createConnection(
    //@ts-ignore
    new BrowserMessageReader(self),
    //@ts-ignore
    new BrowserMessageWriter(self),
  );

  initSync(wasmURL);
  console.log(`tinymist-web ${TinymistLanguageServer.version()} wasm is loaded...`);

  /**
   * The events are stored in the array and will be processed after the response is sent to the client.
   */
  let events: any[] = [];

  /**
   * The bridge between the server in wasm and the client.
   */
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

  // Post-processes server events after sent responses to the client.
  const h = <T>(res: T): T => {
    while (events.length > 0) {
      for (const event of events.splice(0)) {
        bridge.on_event(event);
      }
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
