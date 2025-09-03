import { Detypify, type DetypifySymbol, ortEnv } from "detypify-service";
import van from "vanjs-core";

export type Point = [number, number];
export type Stroke = Point[];

// Configure ONNX runtime for Detypify
ortEnv.wasm.numThreads = 4;
ortEnv.wasm.wasmPaths = "https://cdn.jsdelivr.net/npm/onnxruntime-web@1.17.1/dist/";

export function useDetypifyFilter() {
  // Initialize Detypify for handwriting recognition
  const detypifyPromise = Detypify.create();
  const detypify = van.state<Detypify | undefined>(undefined);
  detypifyPromise.then((d: Detypify) => {
    detypify.val = d;
  });

  const strokes = van.state<Stroke[] | undefined>(undefined);
  const drawCandidates = van.state<DetypifySymbol[] | undefined>();

  // Async computation for draw candidates
  (drawCandidates as { _drawCandidateAsyncNode?: unknown })._drawCandidateAsyncNode = van.derive(
    async () => {
      drawCandidates.val =
        detypify.val && strokes.val && (await detypify.val.candidates(strokes.val, 5));
    },
  );

  return { strokes, drawCandidates };
}
