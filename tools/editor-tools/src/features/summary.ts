import van, { ChildDom, State } from "vanjs-core";
import { stringify as csvStringify } from "csv-stringify/browser/esm/sync";
import {
  requestRevealPath,
  requestSaveFontsExportConfigure,
  saveDataToFile,
} from "../vscode";
import { CopyIcon } from "../icons";
import { startModal } from "../components/modal";
import { base64Decode, base64Encode } from "../utils";
const { div, a, span, code, br, button, form, textarea, label, input } =
  van.tags;

interface ServerInfo {
  root: string;
  fontPaths: string[];
  inputs: Record<string, string>;
  stats: Record<string, string>;
}

type ServerInfoMap = Record<string, ServerInfo>;

export const Summary = () => {
  const documentMetricsData = `:[[preview:DocumentMetrics]]:`;
  const docMetrics = van.state<DocumentMetrics>(
    documentMetricsData.startsWith(":")
      ? DOC_MOCK
      : JSON.parse(base64Decode(documentMetricsData))
  );
  console.log("docMetrics", docMetrics);
  const serverInfoData = `:[[preview:ServerInfo]]:`;
  const serverInfos = van.state<ServerInfoMap>(
    serverInfoData.startsWith(":")
      ? SERVER_INFO_MOCK
      : JSON.parse(base64Decode(serverInfoData))
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
      code("Variant"),
      ": ",
      code(
        font.style === "normal" || !font.style
          ? ""
          : `${humanStyle(font.style)}, `,
        span(
          { title: `Weight ${font.weight || 400}` },
          `${humanWeight(font.weight)} Weight`
        ),
        ", ",
        span(
          { title: `Stretch ${(font.stretch || 1000) / 10}%` },
          `${humanStretch(font.stretch)} Stretch`
        )
      ),
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

      for (const [key, htmlContent] of Object.entries(val.stats)) {
        res.push(
          div(
            div({ href: "javascript:void(0)" }, code(key)),
            div({ innerHTML: htmlContent })
          )
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
        { style: "position: relative; width: 100%; height: 0px" },
        button(
          {
            class: "tinymist-button",
            style: "position: absolute; top: 0px; right: 0px",
            onclick: () => {
              startModal(
                div(
                  {
                    style:
                      "height: calc(100% - 20px); box-sizing: border-box; padding-top: 4px",
                  },
                  fontsExportPanel({
                    fonts: docMetrics.val.fontInfo,
                    sources: docMetrics.val.spanInfo.sources,
                  })
                )
              );
            },
          },
          CopyIcon()
        )
      ),
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
          a({ href: "javascript:void(0)" }, "0.11.6"),
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

interface fontsExportPanelProps {
  fonts: FontInfo[];
  sources: FontSource[];
}

interface fontInfoWithSource extends Omit<FontInfo, "source"> {
  source: FontSource | null;
}

export type fontsCSVHeader =
  | "name"
  | "postscript"
  | "style"
  | "weight"
  | "stretch"
  | "location"
  | "path";

interface csvFieldExtractor<H, T> {
  fieldName: H;
  extractor: (input: T) => string | number;
}

type fontCSVFieldExtractor = csvFieldExtractor<
  fontsCSVHeader,
  fontInfoWithSource
>;

class fontsCSVGenerator {
  public static readonly fieldExtractors: fontCSVFieldExtractor[] = [
    {
      fieldName: "name",
      extractor: (info) => info.fullName ?? "",
    },
    {
      fieldName: "postscript",
      extractor: (info) => info.postscriptName,
    },
    {
      fieldName: "style",
      extractor: (info) => info.style ?? "",
    },
    {
      fieldName: "weight",
      extractor: (info) => info.weight ?? "",
    },
    {
      fieldName: "stretch",
      extractor: (info) => info.stretch ?? "",
    },
    {
      fieldName: "location",
      extractor: (info) => {
        switch (info.source?.kind ?? "") {
          case "fs":
            return "fileSystem";
          case "memory":
            return "memory";
          default:
            return "unknown";
        }
      },
    },
    {
      fieldName: "path",
      extractor: (info) => (info.source?.kind === "fs" ? info.source.path : ""),
    },
  ];

  public generate(
    fonts: fontInfoWithSource[],
    config: fontsExportCSVConfigure
  ): string {
    const fields = fontsCSVGenerator.fieldExtractors.filter((field) =>
      config.fields.includes(field.fieldName)
    );
    const headers = fields.map((field) => field.fieldName);

    let rows = fonts.map((font) =>
      fields.map((field) => field.extractor(font))
    );

    // If only field is file path, do a dedupp
    if (fields.length === 1 && fields[0].fieldName === "path") {
      const dedup = new Set();
      rows = rows.reduce(
        (acc, item) => {
          const path = item[0];
          if (!dedup.has(path)) {
            dedup.add(path);
            acc.push(item);
          }
          return acc;
        },
        [] as typeof rows
      );
    }

    return csvStringify(rows, {
      header: config.header,
      columns: headers,
      delimiter: config.delimiter,
    });
  }
}

export type fontLocation = FontSource extends { kind: infer Kind }
  ? Kind
  : never;

export interface fontsExportCSVConfigure {
  header: boolean;
  delimiter: string;
  fields: fontsCSVHeader[];
}

export interface fontsExportJSONConfigure {
  indent: number;
}

export interface fontsExportFormatConfigure {
  csv: fontsExportCSVConfigure;
  json: fontsExportJSONConfigure;
}

export type fontsExportFormat = keyof fontsExportFormatConfigure;

interface fontsExportCommonConfigure {
  format: fontsExportFormat;
  filters: {
    location: fontLocation[];
  };
}

export type fontsExportConfigure = fontsExportCommonConfigure &
  fontsExportFormatConfigure;

// todo: deduplicate me. it also occurs in editors/vscode/src/editor-tools.ts
export const fontsExportDefaultConfigure: fontsExportConfigure = {
  format: "csv",
  filters: {
    location: ["fs"],
  },
  csv: {
    header: false,
    delimiter: ",",
    fields: ["name", "path"],
  },
  json: {
    indent: 2,
  },
};

let savedConfigureData = `:[[preview:FontsExportConfigure]]:`;

const fontsExportPanel = ({ fonts, sources }: fontsExportPanelProps) => {
  const savedConfigure: fontsExportConfigure = savedConfigureData.startsWith(
    ":"
  )
    ? fontsExportDefaultConfigure
    : JSON.parse(base64Decode(savedConfigureData));

  const exportFormat = van.state<fontsExportFormat>(savedConfigure.format);
  const locationFilter = van.state<fontLocation[]>(
    savedConfigure.filters.location
  );
  const csvConfigure = van.state<fontsExportCSVConfigure>(savedConfigure.csv);
  const jsonConfigure = van.state<fontsExportJSONConfigure>(
    savedConfigure.json
  );

  // Save state when changed
  van.derive(() => {
    const configure: fontsExportConfigure = {
      format: exportFormat.val,
      filters: {
        location: locationFilter.val,
      },
      csv: csvConfigure.val,
      json: jsonConfigure.val,
    };

    savedConfigureData = base64Encode(JSON.stringify(configure));
    requestSaveFontsExportConfigure(configure);
  });

  const data: fontInfoWithSource[] = fonts.map((font) => {
    let source = typeof font.source === "number" ? sources[font.source] : null;
    return Object.assign({}, font, { source });
  });

  const filteredData = van.derive(() => {
    return data.filter((font) =>
      locationFilter.val.includes(font.source?.kind ?? ("" as any))
    );
  });

  const exportText = van.derive<string>(() => {
    switch (exportFormat.val) {
      case "csv": {
        const csvGenerator = new fontsCSVGenerator();
        return csvGenerator.generate(filteredData.val, csvConfigure.val);
      }
      case "json": {
        return JSON.stringify(filteredData.val, null, jsonConfigure.val.indent);
      }
    }
  });

  const titleWidth = 72;
  const rowGap = 8;

  const labelInputGap = 4;
  const itemGap = 10;
  const groupGap = 20;

  const labeledInput = (
    title: string,
    el: HTMLInputElement,
    { labelStyle } = { labelStyle: "" }
  ) =>
    span(
      {
        style: `display: inline-flex; column-gap: ${labelInputGap}px; align-items: center`,
      },
      label({ for: el.id, style: labelStyle }, title),
      el
    );

  const makeArrayCheckbox = (
    id: string,
    value: string,
    state: State<string[]> | string[]
  ) => {
    const checked = Array.isArray(state)
      ? state.includes(value)
      : state.val.includes(value);
    return input({
      id,
      type: "checkbox",
      style: "margin: 0px",
      value,
      checked,
      onchange: (e: any) => {
        if (e.target.checked) {
          Array.isArray(state)
            ? state.push(e.target.value)
            : (state.val = [...state.rawVal, e.target.value]);
        } else {
          if (Array.isArray(state)) {
            let index = state.indexOf(e.target.value);
            if (index !== -1) {
              state.splice(index, 1);
            }
          } else {
            state.val = state.val.filter((v) => v !== e.target.value);
          }
        }
      },
    });
  };

  const filtersUI = () =>
    div(
      { class: "flex-col", style: `row-gap: ${rowGap}px` },
      div(
        { class: "flex-row", style: "align-items: center" },
        div({ style: `width: ${titleWidth}px` }, "Location"),
        div(
          {
            class: "flex-row",
            style: `flex: 1; flex-wrap: wrap; column-gap: ${itemGap}px`,
          },
          labeledInput(
            "FileSystem",
            makeArrayCheckbox("filter-locations-fs", "fs", locationFilter)
          ),
          labeledInput(
            "Memory",
            makeArrayCheckbox(
              "filter-locations-memory",
              "memory",
              locationFilter
            )
          )
        )
      )
    );

  const chooseExportFormatUI = () =>
    div(
      { class: "flex-row", style: "align-items: center" },
      div({ style: `width: ${titleWidth}px` }, "Format"),
      div(
        {
          class: "flex-row",
          style: `flex: 1; flex-wrap: wrap; column-gap: ${itemGap}px`,
        },
        labeledInput(
          "CSV",
          input({
            id: "export-format-csv",
            type: "radio",
            name: "export-format",
            style: "margin: 0px",
            checked: exportFormat.val === "csv",
            onchange: (e) => {
              if (e.target.checked) {
                exportFormat.val = "csv";
              }
            },
          })
        ),
        labeledInput(
          "JSON",
          input({
            id: "export-format-json",
            type: "radio",
            name: "export-format",
            style: "margin: 0px",
            checked: exportFormat.val === "json",
            onchange: (e) => {
              if (e.target.checked) {
                exportFormat.val = "json";
              }
            },
          })
        )
      )
    );

  const csvConfigureUI = () =>
    form(
      {
        class: "flex-col",
        style: `row-gap: ${rowGap}px`,
        onchange: (_e) => {
          csvConfigure.val = Object.assign({}, csvConfigure.val);
        },
        onsubmit: (e) => e.preventDefault(),
      },
      div(
        { class: "flex-row", style: "align-items: center" },
        div({ style: `width: ${titleWidth}px` }, "Settings"),
        div(
          {
            class: "flex-row",
            style: `flex: 1; flex-wrap: wrap; column-gap: ${groupGap}px`,
          },
          labeledInput(
            "Header",
            input({
              id: "csv-header",
              type: "checkbox",
              style: "margin: 0px",
              checked: csvConfigure.val.header,
              onchange: (e) => (csvConfigure.rawVal.header = e.target.checked),
            })
          ),
          labeledInput(
            "Delimiter:",
            input({
              id: "csv-delimiter",
              type: "input",
              style: `width: 40px`,
              value: csvConfigure.val.delimiter,
              oninput: (e) => (csvConfigure.rawVal.delimiter = e.target.value),
              onkeydown: (e) => e.stopPropagation(), // prevent modal window closed by space when input
            })
          )
        )
      ),
      div(
        { class: "flex-row", style: "align-items: center" },
        div({ style: `width: ${titleWidth}px` }, "Fields"),
        div(
          {
            class: "flex-row",
            style: `flex: 1; flex-wrap: wrap; column-gap: ${itemGap}px`,
          },
          ...fontsCSVGenerator.fieldExtractors
            .map((fe) => fe.fieldName)
            .map((field) =>
              labeledInput(
                field,
                makeArrayCheckbox(
                  `csv-field-${field}`,
                  field,
                  csvConfigure.rawVal.fields
                )
              )
            )
        )
      )
    );

  const jsonConfigureUI = () =>
    form(
      {
        onchange: (_e) => {
          jsonConfigure.val = Object.assign({}, jsonConfigure.val);
        },
        onsubmit: (e) => e.preventDefault(),
      },
      div(
        { class: "flex-row", style: "align-items: center" },
        div({ style: `width: ${titleWidth}px` }, "Settings"),
        div(
          {
            class: "flex-row",
            style: `flex: 1; flex-wrap: wrap; column-gap: ${groupGap}px`,
          },
          labeledInput(
            "Indent:",
            input({
              id: "json-indent",
              type: "number",
              style: "width: 40px;",
              min: "0",
              max: "8",
              step: "2",
              value: jsonConfigure.val.indent,
              onchange: (e) =>
                (jsonConfigure.rawVal.indent = parseInt(e.target.value, 10)),
            }),
            { labelStyle: "margin-right: 0.5em" }
          )
        )
      )
    );

  const exportFormatConfigureUI = () => {
    let ui;
    switch (exportFormat.val) {
      case "csv": {
        ui = csvConfigureUI();
        break;
      }
      case "json": {
        ui = jsonConfigureUI();
        break;
      }
    }
    return ui;
  };

  return div(
    {
      class: "flex-col",
      style: `row-gap: ${rowGap}px; width: 100%; height: 100%`,
    },
    filtersUI,
    chooseExportFormatUI,
    exportFormatConfigureUI,
    textarea(
      {
        class: "tinymist-code",
        style:
          "resize: none; width: 100%; flex: 1; white-space: pre; overflow-wrap: normal; overflow-x: scroll",
        readOnly: true,
        onkeydown: (e) => e.stopPropagation(),
      },
      exportText
    ),
    div(
      { style: `display: flex; align-items: center; column-gap:${itemGap}px` },
      button(
        {
          class: "tinymist-button",
          style: "flex: 1",
          onclick: () => {
            const filterName = `${exportFormat.val.toLocaleUpperCase()} file`;
            saveDataToFile({
              data: exportText.val,
              option: {
                filters: {
                  [filterName]: [exportFormat.val],
                },
              },
            });
          },
        },
        "Export"
      ),
      button(
        {
          class: "tinymist-button",
          style: "flex: 1",
          onclick: () => navigator.clipboard.writeText(exportText.val),
        },
        "Copy"
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
  style?: string;
  weight?: number;
  stretch?: number;
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
    stats: {},
  },
};

function almost(value: number, target: number, threshold = 0.01) {
  return Math.abs(value - target) < threshold;
}

function humanStyle(style?: string) {
  if (!style) {
    return "Regular";
  }

  if (style === "italic") {
    return "Italic";
  }

  if (style === "oblique") {
    return "Oblique";
  }

  return `Style ${style}`;
}

function humanWeight(weight?: number) {
  if (!weight) {
    return "Regular";
  }

  if (almost(weight, 100)) {
    return "Thin";
  }

  if (almost(weight, 200)) {
    return "Extra Light";
  }

  if (almost(weight, 300)) {
    return "Light";
  }

  if (almost(weight, 400)) {
    return "Regular";
  }

  if (almost(weight, 500)) {
    return "Medium";
  }

  if (almost(weight, 600)) {
    return "Semibold";
  }

  if (almost(weight, 700)) {
    return "Bold";
  }

  if (almost(weight, 800)) {
    return "Extra Bold";
  }

  if (almost(weight, 900)) {
    return "Black";
  }

  return `Weight ${weight}`;
}

function humanStretch(stretch?: number) {
  if (!stretch) {
    return "Normal";
  }

  if (almost(stretch, 500)) {
    return "Ultra-condensed";
  }

  if (almost(stretch, 625)) {
    return "Extra-condensed";
  }

  if (almost(stretch, 750)) {
    return "Condensed";
  }

  if (almost(stretch, 875)) {
    return "Semi-condensed";
  }

  if (almost(stretch, 1000)) {
    return "Normal";
  }

  if (almost(stretch, 1125)) {
    return "Semi-expanded";
  }

  if (almost(stretch, 1250)) {
    return "Expanded";
  }

  if (almost(stretch, 1500)) {
    return "Extra-expanded";
  }

  if (almost(stretch, 2000)) {
    return "Ultra-expanded";
  }

  return `${stretch}`;
}
