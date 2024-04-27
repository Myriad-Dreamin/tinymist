import inferSymbols from "../../assets/detypify/infer.json";
// @ts-ignore
import modelUrl from "../../assets/detypify/model.onnx";
import { InferenceSession, Tensor, env as ortConfig } from "onnxruntime-web";

ortConfig.wasm.numThreads = 1;
ortConfig.wasm.wasmPaths =
  "https://cdn.jsdelivr.net/npm/onnxruntime-web@1.17.1/dist/";

export type Point = [number, number];
export type Stroke = Point[];
export interface DetypifySymbol {
  names: string[];
  codepoint: number;
}

export class Detypify {
  strokes?: Stroke[];
  dstCanvas: HTMLCanvasElement;
  dstCtx: CanvasRenderingContext2D;
  private constructor(public session: InferenceSession) {
    let dstCanvas = document.createElement("canvas");
    dstCanvas.width = dstCanvas.height = 32;
    let dstCtx = dstCanvas.getContext("2d", { willReadFrequently: true })!;
    dstCtx.fillStyle = "white";
    this.dstCanvas = dstCanvas;
    this.dstCtx = dstCtx;
  }

  static async create() {
    return new Detypify(await InferenceSession.create(modelUrl));
  }

  async candidates(strokes: Stroke[]): Promise<DetypifySymbol[] | undefined> {
    console.log("candidates", this.session, strokes);
    // not loaded or clear
    if (!this.session || !strokes?.length) return [];
    this.drawToDst(strokes);
    // to grayscale
    let dstWidth = this.dstCanvas.width;
    let rgba = this.dstCtx.getImageData(0, 0, dstWidth, dstWidth).data;
    let grey = new Float32Array(rgba.length / 4);
    for (let i = 0; i < grey.length; ++i) {
      grey[i] = rgba[i * 4] == 255 ? 1 : 0;
    }
    // infer
    let tensor = new Tensor("float32", grey, [1, 1, 32, 32]);
    let output = await this.session.run({
      [this.session.inputNames[0]]: tensor,
    });
    let ddd = Array.prototype.slice.call(
      output[this.session.outputNames[0]].data
    );
    // select top K
    let withIdx = ddd.map((x, i) => [x, i]);
    withIdx.sort((a, b) => b[0] - a[0]);

    let result = withIdx.slice(0, 5).map(([_, i]) => inferSymbols[i]);
    console.log("candidates finished", result);
    return result;
  }

  private drawToDst(strokes: Stroke[]) {
    // find rect
    let minX = Infinity;
    let maxX = 0;
    let minY = Infinity;
    let maxY = 0;
    for (let stroke of strokes) {
      for (let [x, y] of stroke) {
        minX = Math.min(minX, x);
        maxX = Math.max(maxX, x);
        minY = Math.min(minY, y);
        maxY = Math.max(maxY, y);
      }
    }

    // normalize
    let dstWidth = this.dstCanvas.width;
    let width = Math.max(maxX - minX, maxY - minY);
    if (width == 0) return;
    width = width * 1.2 + 20;
    let zeroX = (minX + maxX - width) / 2;
    let zeroY = (minY + maxY - width) / 2;
    let scale = dstWidth / width;

    // draw to dstCanvas
    this.dstCtx.fillRect(0, 0, dstWidth, dstWidth);
    this.dstCtx.translate(0.5, 0.5);
    for (let stroke of strokes) {
      this.dstCtx.beginPath();
      for (let [x, y] of stroke) {
        this.dstCtx.lineTo(
          Math.round((x - zeroX) * scale),
          Math.round((y - zeroY) * scale)
        );
      }
      this.dstCtx.stroke();
    }
    this.dstCtx.translate(-0.5, -0.5);
  }
}
