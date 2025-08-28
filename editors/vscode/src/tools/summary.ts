import * as vscode from "vscode";
import { FONTS_EXPORT_CONFIG_VERSION, type Versioned } from "../features/tool";
import { tinymist } from "../lsp";
import { type ExtensionContext, extensionState } from "../state";
import { substituteTemplateString } from "../util";
import { defineEditorTool } from ".";

interface FsFontSource {
  kind: "fs";
  path: string;
}

interface MemoryFontSource {
  kind: "memory";
  name: string;
}

type FontSource = FsFontSource | MemoryFontSource;

export type FontLocation = FontSource extends { kind: infer Kind } ? Kind : never;

export type FontsCSVHeader =
  | "name"
  | "postscript"
  | "style"
  | "weight"
  | "stretch"
  | "location"
  | "path";

export interface FontsExportCSVConfig {
  header: boolean;
  delimiter: string;
  fields: FontsCSVHeader[];
}

export interface FontsExportJSONConfig {
  indent: number;
}

export interface FontsExportFormatConfig {
  csv: FontsExportCSVConfig;
  json: FontsExportJSONConfig;
}

export type FontsExportFormat = keyof FontsExportFormatConfig;

interface FontsExportCommonConfig {
  format: FontsExportFormat;
  filters: {
    location: FontLocation[];
  };
}

export type FontsExportConfig = FontsExportCommonConfig & FontsExportFormatConfig;

// todo: deduplicate me. it also occurs in tools/editor-tools/src/features/summary.ts
const fontsExportDefaultConfigure: FontsExportConfig = {
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

export function getFontsExportConfigure(context: ExtensionContext) {
  const defaultConfigure: Versioned<FontsExportConfig> = {
    version: FONTS_EXPORT_CONFIG_VERSION,
    data: fontsExportDefaultConfigure,
  };

  const configure = context.globalState.get("fontsExportConfigure", defaultConfigure);
  if (configure?.version !== FONTS_EXPORT_CONFIG_VERSION) {
    return defaultConfigure;
  }

  return configure;
}

const waitTimeList = [100, 200, 400, 1000, 1200, 1500, 1800, 2000];

async function fetchSummaryInfo(): Promise<[string | undefined, string | undefined]> {
  const res: [string | undefined, string | undefined] = [undefined, undefined];

  for (const to of waitTimeList) {
    const focusingFile = extensionState.getFocusingFile();
    if (focusingFile === undefined) {
      await vscode.window.showErrorMessage("No focusing typst file");
      return res;
    }

    await work(focusingFile, res);
    if (res[0] && res[1]) {
      break;
    }
    // wait for a bit
    await new Promise((resolve) => setTimeout(resolve, to));
  }

  return res;

  async function work(focusingFile: string, res: [string | undefined, string | undefined]) {
    if (!res[0]) {
      const result = await tinymist.executeCommand("tinymist.getDocumentMetrics", [focusingFile]);
      if (!result) {
        return;
      }
      const docMetrics = JSON.stringify(result);
      res[0] = docMetrics;
    }

    if (!res[1]) {
      const result2 = await tinymist.executeCommand("tinymist.getServerInfo", []);
      if (!result2) {
        return;
      }
      const serverInfo = JSON.stringify(result2);
      res[1] = serverInfo;
    }
  }
}

export default defineEditorTool({
  id: "summary",
  command: {
    command: "tinymist.showSummary",
    title: "Document Summary",
    tooltip: "Show Document Summary",
  },
  title: "Summary",
  showOption: {
    preserveFocus: true,
  },

  transformHtml: async (html, { context }) => {
    const fontsExportConfigure = getFontsExportConfigure(context);
    const fontsExportConfig = JSON.stringify(fontsExportConfigure.data);
    const [docMetrics, serverInfo] = await fetchSummaryInfo();

    if (!docMetrics || !serverInfo) {
      if (!docMetrics) {
        vscode.window.showErrorMessage("No document metrics available");
      }
      if (!serverInfo) {
        vscode.window.showErrorMessage("No server info");
      }

      return;
    }

    return substituteTemplateString(html, {
      ":[[preview:FontsExportConfigure]]:": fontsExportConfig,
      ":[[preview:DocumentMetrics]]:": docMetrics,
      ":[[preview:ServerInfo]]:": serverInfo,
    });
  },
});
