import van from "vanjs-core";
import { traceData } from "../vscode";
const { div, button } = van.tags;

const ORIGIN = "https://ui.perfetto.dev";

const openTrace = (arrayBuffer: ArrayBuffer, traceUrl?: string) => {
  let handle = window.open(ORIGIN)!;

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
  const since = Date.now();
  const collecting = setInterval(() => {
    const message = document.getElementById("message")!;
    const elapsed = Date.now() - since;
    const elapsedAlign = (elapsed / 1000).toFixed(1).padStart(5, " ");

    if (traceData.val) {
      clearInterval(collecting);
      const openTraceButton = document.getElementById(
        "open-trace"
      ) as HTMLButtonElement;
      openTraceButton.style.display = "block";
      message.innerText = `Trace collected... ${elapsedAlign}s`;

      const tracingContent = enc.encode(traceData.val);
      openTrace(tracingContent.buffer);
      return;
    }

    message.innerText = `Collecting trace... ${elapsedAlign}s`;
  }, 100);

  return div(
    {
      class: "flex-col",
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
  );
};
