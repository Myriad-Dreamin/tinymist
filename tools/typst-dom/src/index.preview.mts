export * from "./typst-doc.mjs";
import { provideSvgDoc } from "./typst-doc.svg.mjs";
import { provideCanvasDoc } from "./typst-doc.canvas.mjs";
import { provideDebugJumpDoc } from "./typst-debug-info.mjs";
import { provideOutlineDoc } from "./typst-outline.mjs";
import { TypstDocumentContext, composeDoc, provideDoc } from "./typst-doc.mjs";

export class TypstPreviewDocument extends provideDoc(
  composeDoc(
    TypstDocumentContext,
    provideOutlineDoc,
    provideCanvasDoc,
    provideSvgDoc,
    provideDebugJumpDoc
  )
) {}
