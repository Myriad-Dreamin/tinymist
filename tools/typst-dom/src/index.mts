export * from "./typst-doc.mjs";
import { provideSvgDoc } from "./typst-doc.svg.mjs";
import { provideCanvasDoc } from "./typst-doc.canvas.mjs";
import { TypstDocumentContext, composeDoc, provideDoc } from "./typst-doc.mjs";

// export class TypstDocument extends provideDoc(
//   provideCanvasDoc(TypstDocumentContext)
// ) {}
/**
 * This is the default typst document class
 * If you want to use other features, you can compose your own document class by using `provideDoc` series functions
 *
 * @example
 *
 * Document with only canvas mode rendering:
 * ```ts
 * class MyDocument extends provideDoc(
 *   provideCanvasDoc(
 *     TypstDocumentContext
 *   )
 * ) {}
 */
export class TypstDocument extends provideDoc(
  composeDoc(TypstDocumentContext, provideCanvasDoc, provideSvgDoc)
) {}
