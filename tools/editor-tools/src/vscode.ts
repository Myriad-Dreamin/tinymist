import van from "vanjs-core";
import type { fontsExportConfigure } from "./features/summary";

const vscodeAPI = typeof acquireVsCodeApi !== "undefined" && acquireVsCodeApi();

interface UserActionTraceRequest {
  compilerProgram: string;
  root: string;
  main: string;
  inputs: any;
  fontPaths: string[];
}

export interface LspResponse {
  id: number;
  result: any;
  error: any;
}

export interface LoC {
  line: number;
  character: number;
}

export interface VscodeDiagnostics {
  path: string;
  message: string;
  range: {
    start: LoC;
    end: LoC;
  };
}

export interface LspNotification {
  method: string;
  params: Record<string, VscodeDiagnostics[]>;
}

export type LspMessage = LspResponse | LspNotification;

interface TraceReport {
  request: UserActionTraceRequest;
  messages: LspMessage[];
  stderr: string;
}

export interface StyleAtCursor {
  version: number;
  textDocument: any;
  position: any;
  style: string[];
  styleAt: any[];
}

// import { traceDataMock } from "./vscode.trace.mock";
// export const traceData = van.state<TraceReport | undefined>(traceDataMock);
export const traceData = van.state<TraceReport | undefined>(undefined);

export const styleAtCursor = van.state<StyleAtCursor | undefined>(undefined);

/// A frontend will try to setup a vscode channel if it is running
/// in vscode.
export function setupVscodeChannel() {
  if (vscodeAPI?.postMessage) {
    // Handle messages sent from the extension to the webview
    window.addEventListener("message", (event: any) => {
      switch (event.data.type) {
        case "traceData": {
          traceData.val = event.data.data;
          break;
        }
        case "styleAtCursor": {
          styleAtCursor.val = event.data.data;
        }
      }
    });
  }
}

export function requestSavePackageData(data: any) {
  if (vscodeAPI?.postMessage) {
    vscodeAPI.postMessage({ type: "savePackageData", data });
  }
}

export function requestSaveFontsExportConfigure(data: fontsExportConfigure) {
  if (vscodeAPI?.postMessage) {
    vscodeAPI.postMessage({ type: "saveFontsExportConfigure", data });
  }
}

export function requestInitTemplate(packageSpec: string) {
  if (vscodeAPI?.postMessage) {
    vscodeAPI.postMessage({ type: "initTemplate", packageSpec });
  }
}

export function requestRevealPath(path: string) {
  if (vscodeAPI?.postMessage) {
    vscodeAPI.postMessage({ type: "revealPath", path });
  }
}

export interface TextEdit {
  range?: undefined;
  newText:
    | string
    | {
        kind: "by-mode";
        math?: string;
        markup?: string;
        code?: string;
        rest?: string;
      };
}

export function copyToClipboard(content: string) {
  if (content === undefined) {
    return;
  }

  if (vscodeAPI?.postMessage) {
    vscodeAPI.postMessage({ type: "copyToClipboard", content });
  } else {
    // copy to clipboard
    navigator.clipboard.writeText(content);
  }
}

export function requestTextEdit(edit: TextEdit) {
  if (vscodeAPI?.postMessage) {
    vscodeAPI.postMessage({ type: "editText", edit });
  } else {
    // copy to clipboard
    navigator.clipboard.writeText(
      typeof edit.newText === "string"
        ? edit.newText
        : edit.newText.code || edit.newText.rest || ""
    );
  }
}

export function saveDataToFile({
  data,
  path,
  option,
}: {
  data: string;
  path?: string;
  option?: any;
}) {
  if (vscodeAPI?.postMessage) {
    vscodeAPI.postMessage({ type: "saveDataToFile", data, path, option });
  }
}
