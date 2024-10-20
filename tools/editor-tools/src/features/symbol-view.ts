import "./symbol-view.css";
import van, { State } from "vanjs-core";
// import { SYMBOL_MOCK } from "./symbol-view.mock";
const { div, input, canvas, button, h4, a, p, span } = van.tags;
import MiniSearch from "minisearch";
import { Detypify, DetypifySymbol, ortEnv } from "detypify-service";
import { ContributeIcon, HelpIcon } from "../icons";
import { startModal } from "../components/modal";
import { requestTextEdit } from "../vscode";

// The following code can make the onnxruntime-web totally offline but causes more than 10MB of bundle size.
// @ts-ignore
// import onnxWasmUrl from "../../../../node_modules/onnxruntime-web/dist/ort-wasm-simd.wasm?url";
// ortEnv.wasm.numThreads = 4;
// ortEnv.wasm.simd = true;
// ortEnv.wasm.proxy = false;
// ortEnv.wasm.trace = false;
// ortEnv.wasm.wasmPaths = {
//   "ort-wasm-simd.wasm": onnxWasmUrl,
// };

ortEnv.wasm.numThreads = 4;
ortEnv.wasm.wasmPaths =
  "https://cdn.jsdelivr.net/npm/onnxruntime-web@1.17.1/dist/";

type Point = [number, number];
type Stroke = Point[];

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
    // console.log("search", Object.values(state.val.symbols));
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

const CanvasPanel = (strokesState: State<Stroke[] | undefined>) => {
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

  // todo: decouple with CanvasPanel
  const serverDark = document.body.classList.contains("typst-preview-dark");
  if (serverDark) {
    srcCtx.fillStyle = "white";
    srcCtx.strokeStyle = "white";
  } else {
    srcCtx.fillStyle = "black";
    srcCtx.strokeStyle = "black";
  }

  type Point = [number, number];
  type PointEvent = Pick<MouseEvent, "offsetX" | "offsetY">;

  let isDrawing = false;
  let currP: Point;
  let stroke: Point[];

  const touchCall = (fn: (e: PointEvent) => any) => (e: TouchEvent) => {
    let rect = srcCanvas.getBoundingClientRect();
    fn({
      offsetX: e.touches[0].clientX - rect.left,
      offsetY: e.touches[0].clientY - rect.top,
    });
  };

  function drawStart({ offsetX, offsetY }: PointEvent) {
    isDrawing = true;

    offsetX = Math.round(offsetX);
    offsetY = Math.round(offsetY);

    currP = [offsetX, offsetY];
    stroke = [currP];
  }

  function drawMove({ offsetX, offsetY }: PointEvent) {
    if (!isDrawing) return;

    offsetX = Math.round(offsetX);
    offsetY = Math.round(offsetY);

    srcCtx.beginPath();
    srcCtx.moveTo(currP[0], currP[1]);
    srcCtx.lineTo(offsetX, offsetY);
    srcCtx.stroke();

    currP = [offsetX, offsetY];
    stroke.push(currP);
  }

  function drawEnd() {
    if (!isDrawing) return; // normal mouse leave
    isDrawing = false;
    if (stroke.length === 1) return; // no line
    strokesState.val = [...(strokesState.oldVal || []), stroke];
  }

  function drawClear() {
    strokesState.val = undefined;
    srcCtx.clearRect(0, 0, srcCanvas.width, srcCanvas.height);
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
      div(
        {
          style: "float: right; margin-right: -18px; cursor: pointer;",
          title: `The offline handwritten stroke recognizer is powered by Detypify. Draw a symbol to search for it.`,
          onclick: () => {
            startModal(
              p(
                "The ",
                span(
                  { style: "font-weight: bold; text-decoration: underline" },
                  "offline"
                ),
                " handwritten stroke recognizer is powered by ",
                a(
                  {
                    href: "https://github.com/QuarticCat/detypify",
                  },
                  "Detypify"
                ),
                ". Draw a symbol to search for it."
              ),
              h4("Cannot find some symbols?"),
              p(
                "🔍: Check the supported symbols listed in ",
                a(
                  {
                    href: "https://github.com/QuarticCat/detypify/blob/main/assets/supported-symbols.txt",
                  },
                  "supported-symbols.txt"
                ),
                "."
              ),
              p(
                "❤️‍🔥: Click the ",
                span({ style: "font-style: italic" }, "contribute mode button"),
                " (",
                ContributeIcon(16, true),
                ") and contribute at ",
                a(
                  {
                    href: "https://detypify.quarticcat.com/",
                  },
                  "Detypify"
                ),
                "."
              ),
              p(
                "📝: Report the missing symbol to ",
                a(
                  {
                    href: "https://github.com/QuarticCat/detypify/issues/new",
                  },
                  "GitHub Issues"
                ),
                "."
              ),
              h4("Like it?"),
              p(
                "Give a star🌟 to the ",
                a(
                  {
                    href: "https://github.com/QuarticCat/detypify",
                  },
                  "Detypify"
                ),
                "!"
              )
            );
          },
        },
        HelpIcon()
      ),
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
    value: "control",
    name: "Control",
  },
  {
    value: "space",
    name: "Space",
  },
  {
    value: "delimiter",
    name: "Delimiters",
  },
  {
    value: "punctuation",
    name: "Punctuations",
  },
  {
    value: "accent",
    name: "Accents",
  },
  {
    value: "quote",
    name: "Quotes",
  },
  {
    value: "prime",
    name: "Primes",
  },
  {
    value: "arithmetics",
    name: "Arithmetic operators",
  },
  {
    value: "logic",
    name: "Logic",
  },
  {
    value: "relation",
    name: "Relation operators",
  },
  {
    value: "setTheory",
    name: "Set Theory",
  },
  {
    value: "calculus",
    name: "Calculus",
  },
  {
    value: "functionAndCategoryTheory",
    name: "Function and category theory",
  },
  {
    value: "numberTheory",
    name: "Number Theory",
  },
  {
    value: "algebra",
    name: "algebra",
  },
  {
    value: "geometry",
    name: "Geometry",
  },
  {
    value: "geometry",
    name: "Geometry",
  },
  {
    value: "currency",
    name: "Currency",
  },
  {
    value: "shape",
    name: "Shape",
  },
  {
    value: "arrow",
    name: "Arrow",
  },
  {
    value: "harpoon",
    name: "Harpoon",
  },
  {
    value: "tack",
    name: "Tack",
  },
  {
    value: "greek",
    name: "Greek Letters",
  },
  {
    value: "hebrew",
    name: "Hebrew Letters",
  },
  {
    value: "doubleStruck",
    name: "Double Struck",
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
    value: "misc",
    name: "Miscellaneous",
  },
  {
    value: "emoji",
    name: "Emoji",
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
  const detypifyPromise = Detypify.create();
  const detypify = van.state<Detypify | undefined>(undefined);
  detypifyPromise.then((d: any) => (detypify.val = d));
  const strokes = van.state<Stroke[] | undefined>(undefined);
  const drawCandidates = van.state<DetypifySymbol[] | undefined>();
  (drawCandidates as any)._drawCandidateAsyncNode = van.derive(async () => {
    let candidates;
    if (strokes.val === undefined) candidates = undefined;
    else if (!detypify.val || !strokes.val) candidates = [];
    else candidates = await detypify.val.candidates(strokes.val, 5);
    drawCandidates.val = candidates;
  });

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
    let maskInfo = div();

    let symCellWidth = "36px";

    if (sym.glyphs?.length && sym.glyphs[0].shape) {
      setTimeout(() => {
        let fontSelected = symInfo.val.fontSelects![sym.glyphs[0].fontIndex];
        let primaryGlyph = sym.glyphs[0];
        const path = symbolDefs.querySelector(`#${primaryGlyph.shape}`);

        const diff = (min?: number, max?: number) => {
          if (min === undefined || max === undefined) return 0;
          return Math.abs(max - min);
        };

        const bboxXWidth = diff(primaryGlyph.xMin, primaryGlyph.xMax);
        let xWidth = Math.max(
          bboxXWidth,
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

        // centering-x the symbol
        let xShift = -(primaryGlyph.xMin || 0) + (xWidth - bboxXWidth) / 2;

        // translate(0, ${fontSelected.ascender * fontSelected.unitsPerEm})
        const imageData = `<svg xmlns:xlink="http://www.w3.org/1999/xlink" width="${symWidth}" height="${symHeight}" viewBox="0 0 ${xWidth} ${yWidth}" xmlns="http://www.w3.org/2000/svg" ><g transform="translate(${xShift}, ${yShift}) scale(1, -1)">${path?.outerHTML || ""}</g></svg>`;
        // console.log(sym.typstCode, div({ innerHTML: imageData }));
        maskInfo.setAttribute(
          "style",
          `width: ${symWidth}; height: ${symHeight}; -webkit-mask-image: url('data:image/svg+xml;utf8,${encodeURIComponent(imageData)}'); -webkit-mask-size: auto ${symHeight}; -webkit-mask-repeat: no-repeat; transition: background-color 200ms; background-color: currentColor;`
        );
      }, 1);
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
          const rest = sym.typstCode || "";
          const markup = `#${rest}`;
          // math mode will trim the sym. prefix
          const math = `${rest.startsWith("sym.") ? rest.slice(4) : rest}`;
          requestTextEdit({
            newText: {
              kind: "by-mode",
              math,
              markup,
              rest,
            },
          });
        },
      },
      maskInfo
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

  function pickSymbolsBySearch(
    pickers: { key: string; value: SymbolItem; elem: Element }[],
    filteredPickers: SelectedSymbolItem[] | undefined
  ) {
    if (!filteredPickers) return pickers;
    return pickers.filter((picker) =>
      filteredPickers.some((f) => f.typstCode === picker.key)
    );
  }

  function pickSymbolsByDrawCandidates(
    pickers: { key: string; value: SymbolItem; elem: Element }[],
    drawCandidates: DetypifySymbol[] | undefined
  ) {
    if (drawCandidates === undefined) return pickers;
    if (!drawCandidates.length) return [];
    return pickers.filter((picker) => {
      if (!picker.value.typstCode) return false;
      let c = picker.value.typstCode;
      // remove sym. prefix
      if (c.startsWith("sym.")) c = c.slice(4);

      return drawCandidates.some((f) => f.names.includes(c));
    });
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
      CanvasPanel(strokes)
    ),
    div({ style: "flex: 1;" }, (_dom?: Element) =>
      div(
        ...categorize(
          CATEGORY_INFO,
          pickSymbolsBySearch(
            pickSymbolsByDrawCandidates(pickers.val, drawCandidates.val),
            filteredPickers.val
          )
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
