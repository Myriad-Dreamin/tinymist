import * as vscode from "vscode";
import { tinymist } from "./lsp";
import { getFocusingFile } from "./extension";
import { VirtualConsole } from "./util";

type ExportFormat = "pdf" | "png" | "svg" | "html" | "markdown" | "text" | "query" | "pdfpc";

interface ExportArgs {
  format: ExportFormat | ExportFormat[];
  inputPath: string;
  outputPath: string;
  "pdf.creationTimestamp"?: string | null;
  "png.ppi"?: number;
  fill?: string;
  "png.fill"?: string;
  merged?: boolean;
  "svg.merged"?: boolean;
  "png.merged"?: boolean;
  "merged.gap"?: string;
  "png.merged.gap"?: string;
  "svg.merged.gap"?: string;
  "query.format"?: string;
  "query.outputExtension"?: string;
  "query.strict"?: boolean;
  "query.pretty"?: boolean;
  "query.selector"?: string;
  "query.field"?: string;
  "query.one"?: boolean;
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
    } catch (e: any) {
      vc.writeln(`Typst export task failed: ${err(e)}`);
    } finally {
      closeEmitter.fire(0);
    }
  }

  async function run() {
    const rawFormat = exportArgs.format;
    let formats = typeof rawFormat === "string" ? [rawFormat] : rawFormat;

    const uri = ops.resolveInputPath();
    if (uri === undefined) {
      vc.writeln(`No input path found for ${exportArgs.inputPath}`);
      return;
    }

    for (let format of formats) {
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

const exportOps = (exportArgs: ExportArgs) => ({
  inheritedProp(prop: "merged" | "merged.gap", from: "svg" | "png"): any {
    return exportArgs[`${from}.${prop}`] === undefined
      ? exportArgs[prop]
      : exportArgs[`${from}.${prop}`];
  },
  resolvePageOpts(fmt: "svg" | "png"): any {
    if (this.inheritedProp("merged", fmt)) {
      return {
        merged: {
          gap: this.inheritedProp("merged.gap", fmt),
        },
      };
    }
    return "first";
  },
  resolveInputPath() {
    const inputPath = exportArgs.inputPath;
    if (inputPath === "$focused" || inputPath === undefined) {
      return getFocusingFile();
    }

    return inputPath;
  },
});

const provideFormats = (exportArgs: ExportArgs, ops = exportOps(exportArgs)) => ({
  inheritedProp(prop: "merged" | "merged.gap", from: "svg" | "png"): any {
    return exportArgs[`${from}.${prop}`] === undefined
      ? exportArgs[prop]
      : exportArgs[`${from}.${prop}`];
  },
  resolvePageOpts(fmt: "svg" | "png"): any {
    if (ops.inheritedProp("merged", fmt)) {
      return {
        merged: {
          gap: ops.inheritedProp("merged.gap", fmt),
        },
      };
    }
    return "first";
  },
  pdf: {
    opts() {
      return {
        creationTimestamp: exportArgs["pdf.creationTimestamp"],
      };
    },
    export: tinymist.exportPdf,
  },
  png: {
    opts() {
      return {
        ppi: exportArgs["png.ppi"] || 96,
        fill: exportArgs["png.fill"] || exportArgs["fill"],
        page: ops.resolvePageOpts("png"),
      };
    },
    export: tinymist.exportPng,
  },
  svg: {
    opts() {
      return {
        page: ops.resolvePageOpts("svg"),
      };
    },
    export: tinymist.exportSvg,
  },
  html: {
    opts() {
      return {};
    },
    export: tinymist.exportHtml,
  },
  markdown: {
    opts() {
      return {};
    },
    export: tinymist.exportMarkdown,
  },
  text: {
    opts() {
      return {};
    },
    export: tinymist.exportText,
  },
  query: {
    opts() {
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
    opts() {
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
