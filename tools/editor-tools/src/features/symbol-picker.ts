import "./symbol-picker.css";
import van, { State } from "vanjs-core";
// import { SYMBOL_MOCK } from "./symbol-picker.mock";
const { div, input, canvas, button } = van.tags;
import MiniSearch from "minisearch";

interface SymbolCategory {
  value?: string;
  name: string;
}

interface InstantiatedSymbolCategory {
  value?: string;
  name: string;
  symbols?: InstantiatedSymbolItem[];
}

interface GlyphDesc {
  fontIndex: number;
  xAdvance?: number;
  yAdvance?: number;
  xMin?: number;
  yMin?: number;
  xMax?: number;
  yMax?: number;
  name?: string;
  shape?: string;
}

interface SymbolItem {
  id: string;
  category?: string;
  typstCode?: string;
  categoryHuman?: string;
  unicode: number;
  glyphs: GlyphDesc[];
}

interface FontItem {
  family: String;
  capHeight: number;
  ascender: number;
  descender: number;
  unitsPerEm: number;
}

interface SymbolInformation {
  symbols: Record<string, SymbolItem>;
  fontSelects?: FontItem[];
  glyphDefs?: string;
}
type SelectedSymbolItem = Pick<SymbolItem, "typstCode">;
interface InstantiatedSymbolItem {
  key: string;
  value: SymbolItem;
  elem: Element;
}

const SYMBOL_MOCK: SymbolInformation = {
  symbols: {},
};

const SearchBar = (
  state: State<SymbolInformation>,
  symbolSelected: State<SelectedSymbolItem[] | undefined>
) => {
  const def = MiniSearch.getDefault("tokenize");
  const search = van.derive(() => {
    const search = new MiniSearch({
      fields: ["typstCode", "category", "categoryHuman"],
      storeFields: ["typstCode"],
      tokenize: (string, fieldName) => {
        if (fieldName === "typstCode") {
          const words = string.toLowerCase().split(".");
          if (words[0] === "sym") {
            words.shift();
          }
          return words;
        } else {
          return def(string);
        }
      },
    });
    for (const [key, sym] of Object.entries(state.val.symbols)) {
      sym.id = key;
      sym.categoryHuman = categoryIndex.get(sym.category);
      sym.typstCode = key;
    }
    console.log("search", Object.values(state.val.symbols));
    search.addAll(Object.values(state.val.symbols));
    return search;
  });

  return input({
    class: "tinymist-search-symbol",
    placeholder: "Search symbols...",
    oninput: (e: InputEvent) => {
      const input = e.target as HTMLInputElement;
      if (input.value === "") {
        symbolSelected.val = undefined;
        return;
      }
      const results = search.val.search(input.value, { prefix: true });
      // console.log(input.value, results);
      symbolSelected.val = results.map((r) => ({ typstCode: r.typstCode }));
    },
  });
};

const CanvasPanel = () => {
  const srcCanvas = canvas({
    width: "160",
    height: "160",
    class: "rounded-lg border border-gray-200 bg-gray-100 shadow-md",
  });

  const srcCtx = srcCanvas.getContext("2d")!;
  if (!srcCtx) {
    throw new Error("Could not get context");
  }
  srcCtx.lineWidth = 2;
  srcCtx.lineJoin = "round";
  srcCtx.lineCap = "round";

  const dstCanvas = document.createElement("canvas");
  dstCanvas.width = dstCanvas.height = 32;
  const dstCtx = dstCanvas.getContext("2d", { willReadFrequently: true })!;
  if (!dstCtx) {
    throw new Error("Could not get context");
  }
  dstCtx.fillStyle = "white";

  type Point = [number, number];
  type PointEvent = Pick<MouseEvent, "offsetX" | "offsetY">;

  let isDrawing = false;
  let currP: Point;
  let stroke: Point[];
  let strokes: Point[][] = [];
  let minX = Infinity;
  let minY = Infinity;
  let maxX = 0;
  let maxY = 0;

  const touchCall = (fn: (e: PointEvent) => any) => (e: TouchEvent) => {
    const { left: touchL, top: touchT } = srcCanvas.getBoundingClientRect();
    fn({
      offsetX: e.touches[0].clientX - touchL,
      offsetY: e.touches[0].clientY - touchT,
    });
  };

  function drawStart({ offsetX, offsetY }: PointEvent) {
    isDrawing = true;
    currP = [offsetX, offsetY];
    stroke = [currP!];
  }

  function drawMove({ offsetX, offsetY }: PointEvent) {
    if (!isDrawing) return;

    let prevP = currP;
    currP = [offsetX, offsetY];
    stroke.push(currP);

    srcCtx.strokeStyle = "white";
    srcCtx.beginPath();
    srcCtx.moveTo(...prevP);
    srcCtx.lineTo(...currP);
    srcCtx.stroke();
  }

  function drawEnd() {
    if (!isDrawing) return; // normal mouse leave
    isDrawing = false;

    // update
    strokes.push(stroke);
    let xs = stroke.map((p) => p[0]);
    minX = Math.min(minX, ...xs);
    maxX = Math.max(maxX, ...xs);
    let ys = stroke.map((p) => p[1]);
    minY = Math.min(minY, ...ys);
    maxY = Math.max(maxY, ...ys);

    // normalize
    let dstWidth = dstCanvas.width;
    let width = Math.max(maxX - minX, maxY - minY);
    if (width == 0) return;
    width *= 1.2;
    let zeroX = (maxX + minX) / 2 - width / 2;
    let zeroY = (maxY + minY) / 2 - width / 2;
    let scale = dstWidth / width;

    // draw to dstCanvas
    dstCtx.fillRect(0, 0, dstWidth, dstWidth);
    dstCtx.translate(0.5, 0.5);
    for (let stroke of strokes) {
      dstCtx.beginPath();
      for (let [x, y] of stroke) {
        dstCtx.lineTo(
          Math.round((x - zeroX) * scale),
          Math.round((y - zeroY) * scale)
        );
      }
      dstCtx.stroke();
    }
    dstCtx.translate(-0.5, -0.5);

    // // [debug] download dstCanvas image
    // let img = document.createElement("a");
    // img.href = dstCanvas.toDataURL();
    // img.download = "test.png";
    // img.click();

    // to greyscale
    let rgba = dstCtx.getImageData(0, 0, dstWidth, dstWidth).data;
    let grey = new Float32Array(rgba.length / 4);
    for (let i = 0; i < grey.length; ++i) {
      grey[i] = rgba[i * 4] == 255 ? 1 : 0;
    }
    // greyscale = grey;
  }

  function drawClear() {
    srcCtx.clearRect(0, 0, srcCanvas.width, srcCanvas.height);
    strokes = [];
    minX = minY = Infinity;
    maxX = maxY = 0;
    // greyscale = null;
  }

  srcCanvas.addEventListener("mousedown", drawStart);
  srcCanvas.addEventListener("mousemove", drawMove);
  srcCanvas.addEventListener("mouseup", drawEnd);
  srcCanvas.addEventListener("mouseleave", drawEnd);
  srcCanvas.addEventListener("touchstart", touchCall(drawStart));
  srcCanvas.addEventListener("touchmove", touchCall(drawMove));
  srcCanvas.addEventListener("touchend", drawEnd);
  srcCanvas.addEventListener("touchcancel", drawEnd);

  return div(
    {
      class: "flex-col",
      style: "align-items: center; gap: 5px",
    },
    div(
      {
        class: "tinymist-canvas-panel",
      },
      srcCanvas
    ),
    button(
      {
        class: "absolute right-1 top-1 p-2",
        title: "clear",
        onclick: drawClear,
      },
      "Clear"
    )
  );
};

const CATEGORY_INFO: SymbolCategory[] = [
  {
    value: "greek",
    name: "Greek Letters",
  },
  {
    value: "hebrew",
    name: "Hebrew Letters",
  },
  {
    value: "delimiter",
    name: "Delimiters",
  },
  {
    value: "mathsConstruct",
    name: "Maths Constructs",
  },
  {
    value: "variableSizedSymbol",
    name: "Variable-sized symbols",
  },
  {
    value: "operator",
    name: "Operators and Relations",
  },
  {
    value: "arrow",
    name: "Arrows",
  },
  {
    value: "accent",
    name: "Accents",
  },
  {
    value: "misc",
    name: "Miscellaneous",
  },
  {
    value: "letterStyle",
    name: "Letter Styles",
  },
];
// generate map from category value to category name
const categoryIndex = new Map(
  CATEGORY_INFO.map((cat) => [cat.value, cat.name.toLowerCase()])
);

export const SymbolPicker = () => {
  const symbolInformationData = `:[[preview:SymbolInformation]]:`;
  const symInfo = van.state<SymbolInformation>(
    symbolInformationData.startsWith(":")
      ? SYMBOL_MOCK
      : JSON.parse(atob(symbolInformationData))
  );
  console.log("symbolInformation", symInfo);
  // console.log("symbolInformationEnc", JSON.stringify(symInfo.val));

  const symbolDefs = div({
    innerHTML: van.derive(
      () =>
        `<svg xmlns:xlink="http://www.w3.org/1999/xlink" width="0" height="0" viewBox="0 0 0 0" role="img" focusable="false" xmlns="http://www.w3.org/2000/svg" style="opacity: 0; position: absolute">
  ${symInfo.val.glyphDefs || ""}
  </svg>`
    ),
  });

  const SymbolCell = (sym: SymbolItem) => {
    let maskInfo = "";

    let symCellWidth = "36px";

    if (sym.glyphs?.length && sym.glyphs[0].shape) {
      let fontSelected = symInfo.val.fontSelects![sym.glyphs[0].fontIndex];
      let primaryGlyph = sym.glyphs[0];
      const path = symbolDefs.querySelector(`#${primaryGlyph.shape}`);

      const diff = (min?: number, max?: number) => {
        if (min === undefined || max === undefined) return 0;
        return Math.abs(max - min);
      };

      let xWidth = Math.max(
        diff(primaryGlyph.xMin, primaryGlyph.xMax),
        primaryGlyph.xAdvance || fontSelected.unitsPerEm
      );

      let yReal = diff(primaryGlyph.yMin, primaryGlyph.yMax);
      let yGlobal = primaryGlyph.yAdvance || fontSelected.unitsPerEm;
      let yWidth = Math.max(yReal, yGlobal);

      let symWidth;
      let symHeight;
      if (xWidth < yWidth) {
        // = `${(primaryGlyph.xAdvance / fontSelected.unitsPerEm) * 33}px`;
        symWidth = `${(xWidth / yWidth) * 33}px`;
        symHeight = "33px";
      } else {
        symWidth = "33px";
        symHeight = `${(yWidth / xWidth) * 33}px`;
      }

      let yShift =
        yReal >= yGlobal
          ? Math.abs(primaryGlyph.yMax || 0)
          : (Math.abs(primaryGlyph.yMax || 0) + yWidth) / 2;

      // translate(0, ${fontSelected.ascender * fontSelected.unitsPerEm})
      const imageData = `<svg xmlns:xlink="http://www.w3.org/1999/xlink" width="${symWidth}" height="${symHeight}" viewBox="0 0 ${xWidth} ${yWidth}" xmlns="http://www.w3.org/2000/svg" ><g transform="translate(0, ${yShift}) scale(1, -1)">${path?.outerHTML || ""}</g></svg>`;
      // console.log(sym.typstCode, div({ innerHTML: imageData }));
      maskInfo = `width: ${symWidth}; height: ${symHeight}; -webkit-mask-image: url('data:image/svg+xml;utf8,${encodeURIComponent(imageData)}'); -webkit-mask-size: auto ${symHeight}; -webkit-mask-repeat: no-repeat; transition: background-color 200ms; background-color: currentColor;`;
    }

    return div(
      {
        class: "tinymist-symbol-cell flex-col",
        style: `flex: 0 0 ${symCellWidth}; width: ${symCellWidth}; height: ${symCellWidth}; padding: 1px; display: flex; justify-content: center; align-items: center;`,
        title: sym.typstCode || "unknown",
        onclick() {
          // animation color
          const d = this as HTMLDivElement;
          d.classList.add("active");
          setTimeout(() => d.classList.remove("active"), 500);
          // clipboard
          navigator.clipboard.writeText(sym.typstCode || "");
        },
      },
      maskInfo ? div({ style: maskInfo }) : null
    );
  };

  const CategoryPicker = (cat: InstantiatedSymbolCategory) => {
    return div(
      div({ style: "font-size: 14px; margin: 8px 0" }, cat.name),
      div(
        { class: "flex-row", style: "flex-wrap: wrap; gap: 5px; width: 100%" },
        ...(cat.symbols || []).map((sym) => sym.elem)
      )
    );
  };

  // .map((info) => CategoryPicker(info))
  const pickers = van.derive(() =>
    Object.entries(symInfo.val.symbols).map(([key, value]) => {
      value.typstCode = key;
      return {
        key,
        value,
        elem: SymbolCell(value),
      };
    })
  );
  const filteredPickers = van.state<SelectedSymbolItem[] | undefined>(
    undefined
  );

  function pickSymbols(
    pickers: { key: string; value: SymbolItem; elem: Element }[],
    filteredPickers: SelectedSymbolItem[] | undefined
  ) {
    if (!filteredPickers) return pickers;
    return pickers.filter((picker) =>
      filteredPickers.some((f) => f.typstCode === picker.key)
    );
  }

  return div(
    {
      class: "tinymist-symbol-main",
      style: "align-items: flex-start; gap: 10px",
    },
    div(
      {
        class: "tinymist-symbol-left flex-col",
        style: "flex: 0 0 auto; gap: 5px",
      },
      SearchBar(symInfo, filteredPickers),
      CanvasPanel()
    ),
    div({ style: "flex: 1;" }, (_dom?: Element) =>
      div(
        ...categorize(
          CATEGORY_INFO,
          pickSymbols(pickers.val, filteredPickers.val)
        )
          .filter((cat) => cat.symbols?.length)
          .map((info) => CategoryPicker(info))
      )
    )
  );
};

function categorize(
  catsRaw: SymbolCategory[],
  symInfo: InstantiatedSymbolItem[]
): InstantiatedSymbolCategory[] {
  let cats: InstantiatedSymbolCategory[] = [
    ...catsRaw.map((cat) => ({ ...cat })),
  ];
  // let misc
  let misc: InstantiatedSymbolCategory = cats.find(
    (cat) => cat.name === "Miscellaneous"
  )!;
  // misc.symbols = symInfo.val.symbols;
  for (let sym of symInfo) {
    const { key, value } = sym;
    let targetCat = misc;
    if (value.category) {
      targetCat = cats.find((cat) => cat.value === value.category) || misc;
    }
    let symbols = targetCat.symbols || (targetCat.symbols = []);
    value.typstCode = key;
    symbols.push(sym);
  }

  return cats;
}
