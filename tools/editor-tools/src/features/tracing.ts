import van, { ChildDom } from "vanjs-core";
import {
  LspMessage,
  LspNotification,
  LspResponse,
  traceData as traceReport,
} from "../vscode";
import { startModal } from "../components/modal";
const { div, h2, button, iframe, code, br, span } = van.tags;

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

  const mainWindow = div(
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
  );

  const since = Date.now();
  const collecting = setInterval(async () => {
    const message = document.getElementById("message")!;
    if (!message) {
      return;
    }
    const elapsed = Date.now() - since;
    const elapsedAlign = (elapsed / 1000).toFixed(1).padStart(5, " ");

    if (traceReport.val) {
      // console.log(JSON.stringify(traceReport.val));

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

      if (!firstResponse) {
        msg = "No trace data found";
      } else if (firstResponse.error) {
        msg = `Error: ${firstResponse.error.message}`;
      } else {
        msg = "";
        if (firstResponse.result.tracingData) {
          tracingContent = enc.encode(firstResponse.result.tracingData).buffer;
        } else if (firstResponse.result.tracingUrl) {
          const response = await fetch(firstResponse.result.tracingUrl);
          tracingContent = await response.arrayBuffer();
        } else {
          msg = "No trace data or url found in response";
        }
      }

      if (!firstResponse) {
        message.innerText = "No response found";
        return;
      }

      message.innerText = "";
      mainWindow.style.display = "none";

      startModal(
        div(
          { style: "margin: 1em 0" },
          ...(msg.length > 0 ? [code(msg), br()] : []),
          "Run ",
          diffPath(rep.request.root, rep.request.main),
          " using ",
          shortProgram(rep.request.compilerProgram),
          " in ",
          elapsedAlign.trim(),
          "s, with ",
          code(
            {
              title: decodeStream(rep.stderr),
              style: "text-decoration: underline",
            },
            "logging"
          ),
          ".",
          optionalInputs(rep.request.inputs),
          optionalFontPaths(rep.request.fontPaths)
        ),
        diagReport(diagnosticsMessage?.params) as Node
      );

      if (tracingContent) {
        openTrace(tracingContent);
      }

      return;
    }

    message.innerText = `Collecting trace... ${elapsedAlign}s`;
  }, 100);

  return div(
    mainWindow,
    iframe({
      id: "perfetto",
      style: "display: none; flex: auto; border: none;",
      // sandbox: "allow-same-origin",
    })
  );
};

function decodeStream(stderr: string): string {
  return atob(stderr);
}

function diffPath(root: string, main: string): ChildDom {
  if (main.startsWith(root)) {
  }

  main = main.slice(root.length);

  return code(
    code({ style: "color: #2486b9; text-decoration: underline" }, root),
    code({ style: "color: #8cc269; text-decoration: underline" }, main)
  );
}
function shortProgram(compilerProgram: string): ChildDom {
  let lastPath = compilerProgram.split(/[\/\\]/g).pop();
  if (lastPath) {
    // trim extension
    lastPath = lastPath.replace(/\.[^.]*$/, "");
    return code(
      { title: compilerProgram, style: "text-decoration: underline" },
      lastPath
    );
  }
}
function optionalInputs(inputs: any): ChildDom {
  if (inputs?.length) {
    return div("Inputs: ", code(JSON.stringify(inputs)));
  }

  return div();
}
function optionalFontPaths(fontPaths: string[]): ChildDom {
  if (fontPaths?.length) {
    return code("Font paths: ", code(JSON.stringify(fontPaths)));
  }

  return div();
}
function diagReport(diagnostics?: LspNotification["params"]): ChildDom {
  if (
    !diagnostics ||
    !Object.values(diagnostics)
      .map((d) => d?.length || 0)
      .some((l) => l > 0)
  ) {
    return div();
  }

  const diagDivs: ChildDom[] = [];

  for (const [path, diags] of Object.entries(diagnostics)) {
    if (diags.length === 0) {
      continue;
    }

    const pathDiv = div(
      code(
        { style: "text-decoration: underline", title: path },
        path.split(/[\/\\]/g).pop()
      )
    );

    const diagPre = div(
      diags.map((d, i) =>
        div(
          { style: "margin: 0.5em" },
          ...(i
            ? [
                div({
                  style: "border-top: 1px solid currentColor; margin: 0.5em 0",
                }),
              ]
            : []),
          span(
            span(`${d.range.start.line}:${d.range.start.character}`),
            "-",
            span(`${d.range.end.line}:${d.range.end.character}`),
            " ",
            d.message,
            "\n"
          )
        )
      )
    );

    diagDivs.push(div(pathDiv, diagPre));
  }

  return div(
    { style: "margin-top: 1.5em" },
    h2({ style: "margin: 0.4em 0" }, "Diagnostics"),
    ...diagDivs
  );
}
