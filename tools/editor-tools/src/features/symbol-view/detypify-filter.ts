import type { Detypify, DetypifySymbol } from "detypify-service";
import van from "vanjs-core";

export type Point = [number, number];
export type Stroke = Point[];

export function useDetypifyFilter() {
  // Initialize Detypify for handwriting recognition
  const detypify = van.state<Detypify | undefined>(undefined);

  import("detypify-service").then(({ Detypify, ortEnv }) => {
    // Configure ONNX runtime for Detypify
    ortEnv.wasm.wasmPaths = "https://cdn.jsdelivr.net/npm/onnxruntime-web@1.17.1/dist/";

    Detypify.create().then((d: Detypify) => {
      detypify.val = d;
    });
  });

  const strokes = van.state<Stroke[] | undefined>(undefined);
  const drawCandidates = van.state<DetypifySymbol[] | undefined>();

  // Async computation for draw candidates
  van.derive(async () => {
    drawCandidates.val =
      detypify.val && strokes.val && (await detypify.val.candidates(strokes.val, 5));
  });

  const detypifyAvailable = van.derive(() => !!detypify.val);

  return { detypifyAvailable, strokes, drawCandidates };
}
