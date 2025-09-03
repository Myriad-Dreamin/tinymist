import van from "vanjs-core";
import type { FontItem, GlyphDesc, SymbolItem, SymbolResource } from "./symbols";

const { div } = van.tags;

const SYM_SIZE = 1.75; // in rem

function renderSymbol(
  mask: HTMLElement,
  primaryGlyph: GlyphDesc,
  fontSelected: FontItem,
  path: string,
) {
  const diff = (min?: number | null, max?: number | null) => {
    return Math.abs((max ?? 0) - (min ?? 0));
  };

  const bboxXWidth = diff(primaryGlyph.xMin, primaryGlyph.xMax);
  const xWidth = Math.max(bboxXWidth, primaryGlyph.xAdvance ?? fontSelected.unitsPerEm);

  const yReal = diff(primaryGlyph.yMin, primaryGlyph.yMax);
  const yGlobal = primaryGlyph.yAdvance ?? fontSelected.unitsPerEm;
  const yWidth = Math.max(yReal, yGlobal);

  // Calculate uniform scaling factor based on the maximum dimension to fit within SYM_SIZE
  const scale = SYM_SIZE / Math.max(xWidth, yWidth);
  const symWidth = `${xWidth * scale}rem`;
  const symHeight = `${yWidth * scale}rem`;

  const yShift =
    yReal >= yGlobal
      ? Math.abs(primaryGlyph.yMax ?? 0)
      : (Math.abs(primaryGlyph.yMax ?? 0) + yWidth) / 2;

  // Center the symbol horizontally
  const xShift = -(primaryGlyph.xMin ?? 0) + (xWidth - bboxXWidth) / 2;

  const imageData = `<svg xmlns:xlink="http://www.w3.org/1999/xlink" width="${symWidth}" height="${symHeight}" viewBox="0 0 ${xWidth} ${yWidth}" xmlns="http://www.w3.org/2000/svg">
<g transform="translate(${xShift}, ${yShift}) scale(1, -1)">${path}</g>
</svg>`;

  mask.setAttribute(
    "style",
    `width: ${symWidth};` +
      `height: ${symHeight};` +
      `-webkit-mask-image: url('data:image/svg+xml;utf8,${encodeURIComponent(imageData)}');` +
      `-webkit-mask-size: auto ${symHeight}`,
  );

  return mask;
}

export function prerenderSymbols(symRes: SymbolResource): SymbolItem[] {
  return Object.entries(symRes.symbols).map(([id, sym]) => {
    const primaryGlyph = sym.glyphs[0];
    const mask = div();
    const renderedSym: SymbolItem = {
      id,
      category: sym.category,
      unicode: sym.unicode,
      rendered: primaryGlyph ? mask : undefined,
    };
    if (primaryGlyph?.fontIndex && primaryGlyph?.shape) {
      const fontSelected = symRes.fontSelects[primaryGlyph.fontIndex];
      if (fontSelected) {
        const glyphPath = (primaryGlyph.shape && symRes.glyphDefs[primaryGlyph.shape]) ?? "";
        setTimeout(() => {
          renderSymbol(mask, sym.glyphs[0], fontSelected, glyphPath);
        });
      }
    }
    return renderedSym;
  });
}
