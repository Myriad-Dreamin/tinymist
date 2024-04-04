import van, { ChildDom } from "vanjs-core";
import { requestRevealPath } from "../vscode";
const { div, a, span, code, br } = van.tags;

interface ServerInfo {
  root: string;
  fontPaths: string[];
  inputs: Record<string, string>;
  estimatedMemoryUsage: Record<string, number>;
}

type ServerInfoMap = Record<string, ServerInfo>;

export const Summary = () => {
  const documentMetricsData = `:[[preview:DocumentMetrics]]:`;
  const docMetrics = van.state<DocumentMetrics>(
    documentMetricsData.startsWith(":")
      ? DOC_MOCK
      : JSON.parse(atob(documentMetricsData))
  );
  console.log("docMetrics", docMetrics);
  const serverInfoData = `:[[preview:ServerInfo]]:`;
  const serverInfos = van.state<ServerInfoMap>(
    serverInfoData.startsWith(":")
      ? SERVER_INFO_MOCK
      : JSON.parse(atob(serverInfoData))
  );
  console.log("serverInfos", serverInfos);

  const FontSlot = (font: FontInfo) => {
    let fontName;
    if (typeof font.source === "number") {
      let w = docMetrics.val.spanInfo.sources[font.source];
      let title;
      if (w.kind === "fs") {
        title = w.path;
        fontName = a(
          {
            style:
              "font-size: 1.2em; text-decoration: underline; cursor: pointer;",
            title,
            onclick() {
              if (w.kind === "fs") {
                requestRevealPath(w.path);
              }
            },
          },
          font.name
        );
      } else {
        title = `Embedded: ${w.name}`;
        fontName = span(
          {
            style: "font-size: 1.2em",
            title,
          },
          font.name
        );
      }
    } else {
      fontName = a({ style: "font-size: 1.2em" }, font.name);
    }

    return div(
      { style: "margin: 1.2em; margin-left: 0.5em" },
      fontName,
      " has ",
      font.usesScale,
      " use(s).",
      br(),
      code("PostScriptName"),
      ": ",
      code(font.postscriptName),
      br(),
      code(
        font.fixedFamily === font.family
          ? "Family"
          : "Family (Identified by Typst)"
      ),
      ": ",
      code(
        font.fixedFamily === font.family
          ? font.family
          : `${font.family} (${font.fixedFamily})`
      )
    );
  };

  const ArgSlots = () => {
    const res: ChildDom[] = [];
    let val = serverInfos.val["primary"];
    if (val.root) {
      res.push(
        div(
          a({ href: "javascript:void(0)" }, code("root")),
          ": ",
          code(val.root)
        )
      );
    }

    for (let i = 0; i < val.fontPaths.length; i++) {
      res.push(
        div(
          a({ href: "javascript:void(0)" }, code(`font-path(${i})`)),
          ": ",
          code(val.fontPaths[i])
        )
      );
    }

    if (val.inputs) {
      const codeList: ChildDom[] = [];
      for (const key of Object.keys(val.inputs)) {
        codeList.push(
          span({ style: "color: #DEC76E" }, key),
          span({ style: "color: #7DCFFF" }, "="),
          val.inputs[key]
        );
      }

      res.push(
        div(
          a({ href: "javascript:void(0)" }, code("sys.inputs")),
          ": ",
          code(...codeList)
        )
      );

      for (const [key, usage] of Object.entries(val.estimatedMemoryUsage)) {
        res.push(
          div(a(code(`memoryUsage (${key})`)), ": ", code(humanSize(usage)))
        );
      }
    }

    return res;
  };

  return div(
    {
      class: "flex-col",
      style: "justify-content: center; align-items: center; gap: 10px;",
    },
    div(
      { class: `tinymist-card`, style: "flex: 1; width: 100%; padding: 10px" },
      div(
        van.derive(() => `This document is compiled with following arguments.`)
      ),
      div({ style: "margin: 1.2em; margin-left: 0.5em" }, ...ArgSlots())
    ),
    div(
      { class: `tinymist-card`, style: "flex: 1; width: 100%; padding: 10px" },
      div(
        van.derive(
          () => `This document uses ${docMetrics.val.fontInfo.length} fonts.`
        )
      ),
      (_dom?: Element) =>
        div(
          ...docMetrics.val.fontInfo
            .sort((x, y) => {
              if (x.usesScale === undefined || y.usesScale === undefined) {
                if (x.usesScale === undefined) {
                  return 1;
                }
                if (y.usesScale === undefined) {
                  return -1;
                }

                return x.name.localeCompare(y.name);
              }
              if (x.usesScale !== y.usesScale) {
                return y.usesScale - x.usesScale;
              }
              return x.name.localeCompare(y.name);
            })
            .map(FontSlot)
        )
    ),
    div(
      {
        class: `tinymist-card hidden`,
        style: "flex: 1; width: 100%; padding: 10px",
      },
      div(`The Tinymist service.`),
      div(
        { style: "margin: 0.8em; margin-left: 0.5em" },
        div(
          `Its version is `,
          a({ href: "javascript:void(0)" }, "0.11.2"),
          `.`
        ),
        div(`It is compiled with optimization level `, "3", `.`),
        div(
          `It is connecting to the client `,
          code({ style: "font-style: italic" }, "VSCode 1.87.2"),
          `.`
        )
      )
    ),
    div(
      {
        class: `tinymist-card hidden`,
        style: "flex: 1; width: 100%; padding: 10px",
      },
      div(`The Typst compiler.`),
      div(
        { style: "margin: 0.8em; margin-left: 0.5em" },
        div(
          `Its version is `,
          a({ href: "javascript:void(0)" }, "0.11.0"),
          `.`
        ),
        div(
          `It identifies `,
          a({ href: "javascript:void(0)" }, "374"),
          ` font variants.`
        )
      )
    ),
    div(
      {
        class: `tinymist-card hidden`,
        style: "flex: 1; width: 100%; padding: 10px",
      },
      div(`The Typst formatters.`),
      div(
        { style: "margin: 0.8em; margin-left: 0.5em" },
        div(`It uses typstyle with following configurations.`),
        code(
          { style: "margin-left: 0.5em" },
          a({ href: "javascript:void(0)", style: "color: #DEC76E" }, "columns"),
          span({ style: "color: #7DCFFF" }, "="),
          "120"
        ),
        div(
          `The version of typstyle is `,
          a({ href: "javascript:void(0)" }, "0.11.7"),
          `.`
        ),
        div(
          `The version of typstfmt is `,
          a({ href: "javascript:void(0)" }, "0.2.9"),
          `.`
        )
      )
    )
  );
};

interface SpanInfo {
  sources: FontSource[];
}

interface FsFontSource {
  kind: "fs";
  path: string;
}

interface MemoryFontSource {
  kind: "memory";
  name: string;
}

type FontSource = FsFontSource | MemoryFontSource;

interface AnnotatedContent {
  content: string;
  spanKind: string;
  /// file >=  0, offset,  mapped length
  /// file == -1,  delta,  mapped length
  /// file =  -2,      0, skipped length
  spans: number[];
}

interface FontInfo {
  name: string;
  postscriptName: string;
  family?: string;
  fullName?: string;
  fixedFamily?: string;
  source?: number;
  index?: number;
  usesScale?: number;
  uses?: AnnotatedContent;
}

interface DocumentMetrics {
  spanInfo: SpanInfo;
  fontInfo: FontInfo[];
}

const DOC_MOCK: DocumentMetrics = {
  spanInfo: {
    sources: [
      {
        kind: "fs",
        path: "C:\\Users\\OvO\\work\\assets\\fonts\\SongTi-Regular.ttf",
      },
      {
        kind: "fs",
        path: "C:\\Users\\OvO\\work\\assets\\fonts\\TimesNewRoman-Regular.ttf",
      },
      {
        kind: "fs",
        path: "C:\\Users\\OvO\\work\\assets\\fonts\\MicrosoftYaHei-Regular.ttf",
      },
    ],
  },
  fontInfo: [
    {
      name: "Song Ti",
      postscriptName: "SongTi",
      source: 0,
      usesScale: 3,
    },
    {
      name: "Times New Roman",
      postscriptName: "TimesNewRoman",
      source: 1,
      usesScale: 4,
    },
    {
      name: "Microsoft YaHei",
      postscriptName: "MicrosoftYaHei",
      source: 2,
      usesScale: 2,
    },
  ],
};

const SERVER_INFO_MOCK: ServerInfoMap = {
  primary: {
    root: "C:\\Users\\OvO\\work\\rust\\tinymist",
    fontPaths: [
      "C:\\Users\\OvO\\work\\rust\\tinymist\\assets\\fonts",
      "C:\\Users\\OvO\\work\\assets\\fonts",
    ],
    inputs: {
      theme: "dark",
      context: '{"preview":true}',
    },
    estimatedMemoryUsage: {},
  },
};

function humanSize(size: number) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let unit = 0;
  while (size >= 768 && unit < units.length) {
    size /= 1024;
    unit++;
  }
  return `${size.toFixed(2)} ${units[unit]}`;
}
