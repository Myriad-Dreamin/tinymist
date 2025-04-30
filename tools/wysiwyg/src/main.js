/// Import stylesheets for different components
// todo: refactor them, but we don't touch them in this PR
import "./styles/typst.css";

import { buildWs, setupVscodeChannel } from "./conn";

/// Main entry point of the frontend program.
main();

function main() {
  const wsArgs = retrieveWsArgs();
  const { nextWs } = buildWs();
  window.onload = () => nextWs(wsArgs);
  setupVscodeChannel(nextWs);
}

/// Placeholders for typst-preview program initializing frontend
/// arguments.
function retrieveWsArgs() {
  /// The string `ws://127.0.0.1:23625` is a placeholder
  /// Also, it is the default url to connect to.
  /// Note that we must resolve the url to an absolute url as
  /// the websocket connection requires an absolute url.
  ///
  /// See [WebSocket and relative URLs](https://github.com/whatwg/websockets/issues/20)
  let urlObject = new URL("ws://127.0.0.1:23625", window.location.href);
  /// Rewrite the protocol to websocket.
  urlObject.protocol = urlObject.protocol.replace("https:", "wss:").replace("http:", "ws:");
  if (location.href.startsWith("https://")) {
    urlObject.protocol = urlObject.protocol.replace("ws:", "wss:");
  }

  /// Return a `WsArgs` object.
  return { url: urlObject.href, isContentPreview: false };
}
