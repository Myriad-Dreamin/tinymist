declare module "detypify-service" {
  export { env as ortEnv } from "onnxruntime-web";
  export declare interface DetypifySymbol {
    names: string[];
  }
  export declare type Point = [number, number];
  export declare type Stroke = Point[];
  export declare class Detypify {
    private constructor(session: InferenceSession);
    /**
     * Load ONNX runtime and the model.
     */
    static load(): Promise<Detypify>;
    /**
     * Inference top `k` candidates.
     */
    candidates(strokes: Stroke[], k: number): Promise<DetypifySymbol[]>;
  }
}
