import van from "vanjs-core";
import {
  LspMessage,
  LspNotification,
  LspResponse,
  traceData as traceReport,
} from "../vscode";
const { div, button, iframe } = van.tags;

const ORIGIN = "https://ui.perfetto.dev";

const openTrace = (arrayBuffer: ArrayBuffer, traceUrl?: string) => {
  let subWindow = document.getElementById("perfetto") as HTMLIFrameElement;
  subWindow.src = ORIGIN;
  subWindow.style.display = "block";
  subWindow.style.width = "100%";
  subWindow.style.height = "100vh";
  let handle = subWindow.contentWindow!;

  //   const btnFetch = document.createElement("button");
  const btnFetch = document.getElementById("open-trace") as HTMLButtonElement;

  if (!handle) {
    btnFetch.classList.add("warning");
    btnFetch.onclick = () => openTrace(arrayBuffer);
    console.log("Popups blocked, you need to manually click the button");
    btnFetch.innerText = "Popups blocked, click here to open the trace file";
    return;
  }

  const timer = setInterval(() => handle.postMessage("PING", ORIGIN), 50);

  const onMessageHandler = (evt: MessageEvent<any>) => {
    if (evt.data !== "PONG") return;

    // We got a PONG, the UI is ready.
    window.clearInterval(timer);
    window.removeEventListener("message", onMessageHandler);

    const reopenUrl = new URL(location.href);
    if (traceUrl) {
      reopenUrl.hash = `#reopen=${traceUrl}`;
    }
    handle.postMessage(
      {
        perfetto: {
          buffer: arrayBuffer,
          title: "Typst Tracing",
          url: reopenUrl.toString(),
        },
      },
      ORIGIN
    );
  };

  window.addEventListener("message", onMessageHandler);
};

const enc = new TextEncoder();

export const Tracing = () => {
  // #tinymist-app.no-wrap
  document.getElementById("tinymist-app")?.classList.add("no-wrap");

  const since = Date.now();
  const collecting = setInterval(() => {
    const message = document.getElementById("message")!;
    const elapsed = Date.now() - since;
    const elapsedAlign = (elapsed / 1000).toFixed(1).padStart(5, " ");

    if (traceReport.val) {
      clearInterval(collecting);
      const openTraceButton = document.getElementById(
        "open-trace"
      ) as HTMLButtonElement;
      openTraceButton.style.display = "block";
      const rep = traceReport.val;

      // find first response
      const firstResponse = rep.messages.find<LspResponse>(
        (msg: LspMessage): msg is LspResponse => "id" in msg && msg.id === 0
      );

      const diagnosticsMessage = rep.messages.find<LspNotification>(
        (msg: LspMessage): msg is LspNotification =>
          "method" in msg && msg.method === "tinymistExt/diagnostics"
      );

      let msg: string;
      let tracingContent: ArrayBuffer | undefined = undefined;
      let diagnostics: string = diagnosticsMessage
        ? `Diagnostics: ${JSON.stringify(diagnosticsMessage.params)}\n`
        : ``;

      if (!firstResponse) {
        msg = "No trace data found";
      } else if (firstResponse.error) {
        msg = `Error: ${firstResponse.error.message}`;
      } else {
        msg = "Trace collected";
        tracingContent = enc.encode(firstResponse.result.tracingData).buffer;
      }

      msg = firstResponse ? "Trace collected" : "No trace data found";

      if (!firstResponse) {
        message.innerText = "No response found";
        return;
      }

      message.innerText = `${msg}... ${elapsedAlign}s
Using program: ${rep.request.compilerProgram}
Root: ${rep.request.root}
Main file: ${rep.request.main}
Inputs: ${JSON.stringify(rep.request.inputs)}
Font paths: ${JSON.stringify(rep.request.fontPaths)}
>>> Stderr
${decodeStream(rep.stderr)}<<< Stderr
${diagnostics}`;

      if (tracingContent) {
        openTrace(tracingContent);
      }

      return;
    }

    message.innerText = `Collecting trace... ${elapsedAlign}s`;
  }, 100);

  return div(
    div(
      {
        class: "flex-col tinymist-main-window",
        style: "justify-content: center;align-items: center;",
      },
      div(
        {
          id: "message",
          style: "flex: auto",
        },
        "Collecting trace..."
      ),
      button({
        id: "open-trace",
        class: "tinymist-button",
        style: "display: none; flex: auto",
      })
    ),
    iframe({
      id: "perfetto",
      style: "display: none; flex: auto",
      // sandbox: "allow-same-origin",
    })
  );
};
function decodeStream(stderr: string): string {
  return atob(stderr);
}
