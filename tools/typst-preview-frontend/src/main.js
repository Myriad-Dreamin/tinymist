
/// Import stylesheets for different components
// todo: refactor them, but we don't touch them in this PR
import "./typst.css";
import "./styles/toolbar.css";
import "./styles/layout.css";
import "./styles/help-panel.css";
import "./styles/outline.css";

import { wsMain, PreviewMode } from './ws';
import { setupDrag } from './drag';

/// Main entry point of the frontend program.
main();

function main() {
    const wsArgs = retrieveWsArgs();
    const { nextWs } = buildWs();
    window.onload = () => nextWs(wsArgs);
    setupVscodeChannel(nextWs);
    setupDrag();
}

/// Placeholders for typst-preview program initializing frontend
/// arguments.
function retrieveWsArgs() {
    /// The string `preview-arg:previewMode:Doc` is a placeholder
    /// It will be replaced by the actual preview mode.
    /// ```rs
    ///   let frontend_html = frontend_html.replace(
    ///     "preview-arg:previewMode:Doc", ...);
    /// ```
    let mode = 'preview-arg:previewMode:Doc';
    /// Remove the placeholder prefix.
    mode = mode.replace('preview-arg:previewMode:', '');
    let previewMode = PreviewMode[mode];

    /// The string `ws://127.0.0.1:23625` is a placeholder
    /// Also, it is the default url to connect to.
    let url = "ws://127.0.0.1:23625";

    /// Return a `WsArgs` object.
    return { url, previewMode, isContentPreview: false };
}

/// `buildWs` returns a object, which keeps track of websocket
///  connections.
function buildWs() {
    let previousDispose = Promise.resolve(() => { });
    /// `nextWs` will always hold a global unique websocket connection
    /// to the preview backend.
    function nextWs(nextWsArgs) {
        const previous = previousDispose;
        previousDispose = new Promise(async (resolve) => {
            /// Dispose the previous websocket connection.
            await previous.then(d => d());
            /// Reset app mode before creating a new websocket connection.
            resetAppMode(nextWsArgs);
            /// Create a new websocket connection.
            resolve(wsMain(nextWsArgs));
        });
    }

    return { nextWs };

    function resetAppMode({ previewMode: mode, isContentPreview }) {
        const app = document.getElementById('typst-container');

        /// Set the root css selector to the content preview mode.
        app.classList.remove('content-preview');
        if (isContentPreview) {
            app.classList.add('content-preview');
        }

        /// Set the root css selector to the preview mode.
        app.classList.remove('mode-slide');
        app.classList.remove('mode-doc');
        if (mode === PreviewMode.Slide) {
            app.classList.add('mode-slide');
        } else if (mode === PreviewMode.Doc) {
            app.classList.add('mode-doc');
        } else {
            throw new Error(`Unknown preview mode: ${mode}`);
        }
    }
}

/// A frontend will try to setup a vscode channel if it is running
/// in vscode.
function setupVscodeChannel(nextWs) {
    const vscodeAPI = (typeof acquireVsCodeApi !== 'undefined') && acquireVsCodeApi();
    if (vscodeAPI?.postMessage) {
        vscodeAPI.postMessage({ type: 'started' });
    }

    // Handle messages sent from the extension to the webview
    window.addEventListener('message', event => {
        const message = event.data; // The json data that the extension sent
        switch (message.type) {
            case 'reconnect': {
                console.log('reconnect', message);
                nextWs({
                    url: message.url,
                    previewMode: PreviewMode[message.mode],
                    isContentPreview: message.isContentPreview,
                });
                break;
            }
            case 'outline': {
                console.log('outline', message);
                break;
            }
        }
    });
}
