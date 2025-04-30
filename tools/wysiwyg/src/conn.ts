import { WsArgs, wsMain } from "./ws";

/// `buildWs` returns a object, which keeps track of websocket
///  connections.
export function buildWs() {
  let previousDispose = Promise.resolve(() => {});
  /// `nextWs` will always hold a global unique websocket connection
  /// to the preview backend.
  function nextWs(nextWsArgs: WsArgs) {
    const previous = previousDispose;
    previousDispose = new Promise(async (resolve) => {
      /// Dispose the previous websocket connection.
      await previous.then((d) => d());
      /// Reset app mode before creating a new websocket connection.
      resetAppMode(nextWsArgs);
      /// Create a new websocket connection.
      resolve(wsMain(nextWsArgs));
    });
  }

  return { nextWs };

  function resetAppMode({}) {
    const app = document.getElementById("typst-container")!;

    /// Set the root css selector to the content preview mode.
    app.classList.remove("content-preview");
  }
}

/// A frontend will try to setup a vscode channel if it is running
/// in vscode.
export function setupVscodeChannel(nextWs: (args: WsArgs) => void) {
  const vscodeAPI = typeof acquireVsCodeApi !== "undefined" && acquireVsCodeApi();
  if (vscodeAPI?.postMessage) {
    vscodeAPI.postMessage({ type: "started" });
  }
  if (vscodeAPI?.setState && window.vscode_state) {
    vscodeAPI.setState(window.vscode_state);
  }

  // Handle messages sent from the extension to the webview
  window.addEventListener("message", (event) => {
    const message = event.data; // The json data that the extension sent
    switch (message.type) {
      case "reconnect": {
        console.log("reconnect", message);
        nextWs({
          url: message.url,
        });
        break;
      }
      case "outline": {
        console.log("outline", message);
        break;
      }
    }
  });
}
