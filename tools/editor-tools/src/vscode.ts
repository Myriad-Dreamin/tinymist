const vscodeAPI = typeof acquireVsCodeApi !== "undefined" && acquireVsCodeApi();

/// A frontend will try to setup a vscode channel if it is running
/// in vscode.
export function setupVscodeChannel() {
  if (vscodeAPI?.postMessage) {
    // Handle messages sent from the extension to the webview
    window.addEventListener("message", (event: any) => {
      void event;
    });
  }
}

export function requestSavePackageData(data: any) {
  if (vscodeAPI?.postMessage) {
    vscodeAPI.postMessage({ type: "savePackageData", data });
  }
}

export function requestInitTemplate(packageSpec: string) {
  if (vscodeAPI?.postMessage) {
    vscodeAPI.postMessage({ type: "initTemplate", packageSpec });
  }
}
