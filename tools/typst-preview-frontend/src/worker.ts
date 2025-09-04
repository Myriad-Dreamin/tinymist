// import {
//     rendererBuildInfo,
//     createTypstRenderer,
//     TypstWorker,
//   } from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";
import mainScript from "@myriaddreamin/typst.ts/dist/esm/main.bundle.js?url";
import renderModule from "@myriaddreamin/typst-ts-renderer/pkg/typst_ts_renderer_bg.wasm?url";

const m = import(/* @vite-ignore */ mainScript);
const w = fetch(renderModule);
let renderer: any = m.then((m) => {
  const r = m.createTypstRenderer();
  return r.init({ beforeBuild: [], getModule: () => w }).then((_: any) => r.workerBridge());
});

let blobIdx = 0;
let blobs = new Map();

(self as any).loadSvg = function (data: any, format: any, w: any, h: any) {
  return new Promise((resolve) => {
    blobIdx += 1;
    blobs.set(blobIdx, resolve);
    postMessage(
      { exception: "loadSvg", token: { blobIdx }, data, format, w, h },
      { transfer: [data.buffer] },
    );
  });
};

onmessage = async function recvMsg({ data }: any) {
  if (data[0] && data[0].blobIdx) {
    let blobResolve = blobs.get(data[0].blobIdx);
    if (blobResolve) {
      blobResolve(data[1]);
    }
    return;
  }
  const r = await renderer;
  requestAnimationFrame(async () => {
    await r.send(data);
  });
};
