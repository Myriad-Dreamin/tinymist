// @ts-ignore
// import { RenderSession as RenderSession2 } from "@myriaddreamin/typst-ts-renderer/pkg/wasm-pack-shim.mjs";
import { RenderSession } from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";
import { WebSocketSubject, webSocket } from "rxjs/webSocket";
import { Subject, Subscription, buffer, debounceTime, tap } from "rxjs";

// for debug propose
// queryObjects((window as any).TypstRenderSession);
(window as any).TypstRenderSession = RenderSession;
// (window as any).TypstRenderSessionKernel = RenderSession2;

export interface WsArgs {
  url: string;
}

export async function wsMain({ url }: WsArgs) {
  if (!url) {
    const hookedElem = document.getElementById("typst-app");
    if (hookedElem) {
      hookedElem.innerHTML = "";
    }
    return () => {};
  }

  let disposed = false;
  let $ws: WebSocketSubject<ArrayBuffer> | undefined = undefined;
  const subsribes: Subscription[] = [];

  function setupSocket(svgDoc: WysiwygDocument): () => void {
    // todo: reconnect setTimeout(() => setupSocket(svgDoc), 1000);
    $ws = webSocket<ArrayBuffer>({
      url,
      binaryType: "arraybuffer",
      serializer: (t) => t,
      deserializer: (event) => event.data,
      openObserver: {
        next: (e) => {
          const sock = e.target;
          console.log("WebSocket connection opened", sock);
          window.typstWebsocket = sock as any;
          svgDoc.reset();
          window.typstWebsocket.send("current");
        },
      },
      closeObserver: {
        next: (e) => {
          console.log("WebSocket connection closed", e);
          $ws?.unsubscribe();
          if (!disposed) {
            setTimeout(() => setupSocket(svgDoc), 1000);
          }
        },
      },
    });

    const batchMessageChannel = new Subject<ArrayBuffer>();

    const dispose = () => {
      disposed = true;
      svgDoc.dispose();
      for (const sub of subsribes.splice(0, subsribes.length)) {
        sub.unsubscribe();
      }
      $ws?.complete();
    };

    // window.typstWebsocket = new WebSocket("ws://127.0.0.1:23625");

    $ws.subscribe({
      next: (data) => batchMessageChannel.next(data), // Called whenever there is a message from the server.
      error: (err) => console.log("WebSocket Error: ", err), // Called if at any point WebSocket API signals some kind of error.
      complete: () => console.log("complete"), // Called when connection is closed (for whatever reason).
    });

    subsribes.push(
      batchMessageChannel
        .pipe(buffer(batchMessageChannel.pipe(debounceTime(0))))
        .pipe(
          tap((dataList) => {
            console.log(`batch ${dataList.length} messages`);
          }),
        )
        .subscribe((dataList) => {
          dataList.map(processMessage);
        }),
    );

    function processMessage(data: ArrayBuffer) {
      console.log(data);
    }

    return dispose;
  }

  return new Promise<() => void>((resolveDispose) => {
    const wsDispose = setupSocket(new WysiwygDocument());

    // todo: plugin init and setup socket at the same time
    resolveDispose(() => {
      // dispose ws first
      wsDispose();
      // dispose kernel then
    });
  });
}

class WysiwygDocument {
  reset() {}
  dispose() {}
}
