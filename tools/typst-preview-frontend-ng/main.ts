import rendererWasmUrl from "@myriaddreamin/typst-ts-renderer/pkg/typst_ts_renderer_bg.wasm?url";
import { startPreviewApp } from "./src/app";
import "./styles.css";

startPreviewApp({ rendererWasmUrl });
