/** biome-ignore-all lint/complexity/useLiteralKeys: special keys */
import * as vscode from "vscode";
import type {
  ExportHtmlOpts,
  ExportPdfOpts,
  ExportPngOpts,
  ExportQueryOpts,
  ExportSvgOpts,
  ExportTextOpts,
  ExportTypliteOpts,
} from "../cmd.export";
import { tinymist } from "../lsp";
import { extensionState } from "../state";
import { VirtualConsole } from "../util";

export type ExportFormat = "pdf" | "png" | "svg" | "html" | "markdown" | "text" | "query" | "pdfpc";

export interface ExportArgs {
  format: ExportFormat | ExportFormat[];
  inputPath: string;
  outputPath: string;
  inputs: [string, string][];

  pages?: string | string[]; // Array of page ranges like ["1-3", "5", "7-9"], or comma separated ranges
  "pdf.pages"?: string | string[];
  "png.pages"?: string | string[];
  "svg.pages"?: string | string[];

  pageNumberTemplate?: string;
  "png.pageNumberTemplate"?: string;
  "svg.pageNumberTemplate"?: string;

  merged?: boolean;
  "svg.merged"?: boolean;
  "png.merged"?: boolean;

  "merged.gap"?: string;
  "png.merged.gap"?: string;
  "svg.merged.gap"?: string;

  "pdf.creationTimestamp"?: string | null;
  "pdf.pdfVersion"?: string;
  "pdf.pdfValidator"?: string;
  "pdf.pdfStandard"?: string[];
  "pdf.pdfTags"?: boolean;

  "png.ppi"?: number;

  fill?: string;
  "png.fill"?: string;

  "query.format": string;
  "query.outputExtension"?: string;
  "query.strict"?: boolean;
  "query.pretty"?: boolean;
  "query.selector": string;
  "query.field"?: string;
  "query.one"?: boolean;

  processor?: string;
  "markdown.processor"?: string;
  "tex.processor"?: string;
  assetsPath?: string;
  "markdown.assetsPath"?: string;
  "tex.assetsPath"?: string;
}

export const runExport = (def: vscode.TaskDefinition) => {
  const exportArgs: ExportArgs = def?.export || {};
  const ops = exportOps(exportArgs);
  const formatProvider = provideFormats(exportArgs);

  const vc = new VirtualConsole();
  const closeEmitter = new vscode.EventEmitter<number>();
  return Promise.resolve({
    onDidWrite: vc.writeEmitter.event,
    onDidClose: closeEmitter.event,
    open,
    close() {},
  });

  async function open() {
    vc.writeln(`Typst export task ${obj(def)}`);

    try {
      await run();
    } catch (e) {
      vc.writeln(`Typst export task failed: ${err(e)}`);
    } finally {
      closeEmitter.fire(0);
    }
  }

  async function run() {
    const rawFormat = exportArgs.format;
    const formats = typeof rawFormat === "string" ? [rawFormat] : rawFormat;

    const uri = ops.resolveInputPath();
    if (uri === undefined) {
      vc.writeln(`No input path found for ${exportArgs.inputPath}`);
      return;
    }

    for (const format of formats) {
      const provider = formatProvider[format];
      if (!provider) {
        vc.writeln(`Unsupported export format: ${format}`);
        continue;
      }

      const extraOpts = provider.opts();
      vc.writeln(`Exporting to ${format}... ${obj(extraOpts)}`);
      const outputPath = await provider.export(uri, extraOpts);
      vc.writeln(`Exported to ${outputPath}`);
    }
  }
};

export const exportOps = (exportArgs: ExportArgs) => ({
  inheritedProp<P extends keyof ExportArgs>(prop: P, from: ExportFormat): ExportArgs[P] {
    const key = `${from}.${prop}` as keyof ExportArgs;
    return exportArgs[key] === undefined ? exportArgs[prop] : (exportArgs[key] as ExportArgs[P]);
  },
  resolvePagesOpts(fmt: "pdf" | "png" | "svg") {
    const pages = this.inheritedProp("pages", fmt);
    return typeof pages === "string" ? pages.split(",") : pages;
  },
  resolveMergeOpts(fmt: "png" | "svg") {
    if (this.inheritedProp("merged", fmt)) {
      return {
        gap: this.inheritedProp("merged.gap", fmt),
      };
    }
  },
  resolveInputPath() {
    const inputPath = exportArgs.inputPath;
    if (inputPath === "$focused" || inputPath === undefined) {
      return extensionState.getFocusingFile();
    }

    return inputPath;
  },
  resolveCommonOpts() {
    return { inputs: exportArgs.inputs };
  },
});

export const provideFormats = (exportArgs: ExportArgs, ops = exportOps(exportArgs)) => ({
  pdf: {
    opts(): ExportPdfOpts {
      const rawCreationTimestamp = exportArgs["pdf.creationTimestamp"];
      const creationTimestamp = rawCreationTimestamp?.includes("T")
        ? Math.floor(new Date(rawCreationTimestamp).getTime() / 1000).toString() // datetime-local to unix timestamp
        : rawCreationTimestamp; // already unix timestamp or null/undefined

      const pdfStandard = exportArgs["pdf.pdfStandard"] ?? [
        ...(exportArgs["pdf.pdfVersion"] ? [exportArgs["pdf.pdfVersion"]] : []),
        ...(exportArgs["pdf.pdfValidator"] ? [exportArgs["pdf.pdfValidator"]] : []),
      ]; // combine version and validator into array

      return {
        pages: ops.resolvePagesOpts("pdf"),
        creationTimestamp,
        pdfStandard,
        noPdfTags: !exportArgs["pdf.pdfTags"], // invert to noPdfTags
      };
    },
    export: tinymist.exportPdf,
  },
  png: {
    opts(): ExportPngOpts {
      return {
        pages: ops.resolvePagesOpts("png"),
        pageNumberTemplate:
          exportArgs["png.pageNumberTemplate"] ?? exportArgs["pageNumberTemplate"],
        merge: ops.resolveMergeOpts("png"),
        ppi: exportArgs["png.ppi"],
        fill: exportArgs["png.fill"] ?? exportArgs["fill"],
      };
    },
    export: tinymist.exportPng,
  },
  svg: {
    opts(): ExportSvgOpts {
      return {
        pages: ops.resolvePagesOpts("svg"),
        pageNumberTemplate:
          exportArgs["svg.pageNumberTemplate"] ?? exportArgs["pageNumberTemplate"],
        merge: ops.resolveMergeOpts("svg"),
      };
    },
    export: tinymist.exportSvg,
  },
  html: {
    opts(): ExportHtmlOpts {
      return {};
    },
    export: tinymist.exportHtml,
  },
  markdown: {
    opts(): ExportTypliteOpts {
      return {
        processor: exportArgs["markdown.processor"] ?? exportArgs["processor"],
        assetsPath: exportArgs["markdown.assetsPath"] ?? exportArgs["assetsPath"],
      };
    },
    export: tinymist.exportMarkdown,
  },
  tex: {
    opts(): ExportTypliteOpts {
      return {
        processor: exportArgs["tex.processor"] ?? exportArgs["processor"],
        assetsPath: exportArgs["tex.assetsPath"] ?? exportArgs["assetsPath"],
      };
    },
    export: tinymist.exportTeX,
  },
  text: {
    opts(): ExportTextOpts {
      return {};
    },
    export: tinymist.exportText,
  },
  query: {
    opts(): ExportQueryOpts {
      return {
        format: exportArgs["query.format"],
        outputExtension: exportArgs["query.outputExtension"],
        strict: exportArgs["query.strict"],
        pretty: exportArgs["query.pretty"],
        selector: exportArgs["query.selector"],
        field: exportArgs["query.field"],
        one: exportArgs["query.one"],
      };
    },
    export: tinymist.exportQuery,
  },
  pdfpc: {
    opts(): ExportQueryOpts {
      return {
        format: "json",
        pretty: exportArgs["query.pretty"],
        outputExtension: "pdfpc",
        selector: "<pdfpc-file>",
        field: "value",
        one: true,
      };
    },
    export: tinymist.exportQuery,
  },
});

function obj(obj: any): string {
  return JSON.stringify(obj, null, 1);
}

const err = (e: any) =>
  obj({
    code: e.code,
    message: e.message,
    stack: e.stack,
    error: e,
  });
