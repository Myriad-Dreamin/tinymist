import * as vscode from "vscode";
import { tinymist } from "./lsp";
import { getFocusingFile } from "./extension";

// This ends up as the `type` key in tasks.json. RLS also uses `typst` and
// our configuration should be compatible with it so use the same key.
export const TYPST_TASK_TYPE = "typst";

export const TYPST_TASK_SOURCE = "typst";

export function activateTaskProvider(context: vscode.ExtensionContext): vscode.Disposable {
  const provider = new TypstTaskProvider(context);
  return vscode.tasks.registerTaskProvider(TYPST_TASK_TYPE, provider);
}

export type ExportFormat = "pdf" | "png" | "svg";

export type TaskDefinition = vscode.TaskDefinition & {
  readonly type: typeof TYPST_TASK_TYPE;
  command: "export";
  export: {
    format: ExportFormat | ExportFormat[];
    inputPath: string;
    outputPath: string;
    "pdf.creationTimestamp"?: string | null;
    "png.ppi"?: number;
    merged?: boolean;
    "merged.gap"?: string;
  };
};

export type TypstTask = vscode.Task & {
  definition: TaskDefinition;
};

function isTypstTask(task: vscode.Task): task is TypstTask {
  return task.definition.type === TYPST_TASK_TYPE;
}

class TypstTaskProvider implements vscode.TaskProvider {
  constructor(private readonly context: vscode.ExtensionContext) {}

  async provideTasks(): Promise<vscode.Task[]> {
    return [];
  }

  async resolveTask(task: vscode.Task): Promise<vscode.Task | undefined> {
    if (isTypstTask(task)) {
      if (task.definition.command === "export") {
        const resolved = new vscode.Task(
          task.definition,
          task.scope || vscode.TaskScope.Workspace,
          task.name,
          TYPST_TASK_SOURCE,
          await callTypstExportCommand(),
        );
        resolved.group = vscode.TaskGroup.Build;
        return resolved;
      }
    }

    return task;
  }
}

export async function callTypstExportCommand(): Promise<vscode.CustomExecution> {
  return new vscode.CustomExecution((def) => {
    const definition = def as TaskDefinition;
    const exportArgs = definition?.export || {};
    const writeEmitter = new vscode.EventEmitter<string>();
    const closeEmitter = new vscode.EventEmitter<number>();

    const formatProvider = {
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
            page: resolvePageOpts(),
          };
        },
        export: tinymist.exportPng,
      },
      svg: {
        opts() {
          return {
            page: resolvePageOpts(),
          };
        },
        export: tinymist.exportSvg,
      },
    };

    return Promise.resolve({
      onDidWrite: writeEmitter.event,
      onDidClose: closeEmitter.event,
      async open() {
        writeEmitter.fire("Typst export task " + obj(definition) + "\r\n");

        try {
          await run();
        } catch (e) {
          writeEmitter.fire("Typst export task failed: " + obj(e) + "\r\n");
        } finally {
          closeEmitter.fire(0);
        }
      },
      close() {
        console.log("Typst export task custom execution close", definition);
      },
    });

    async function run() {
      const rawFormat = exportArgs.format;
      let formats = typeof rawFormat === "string" ? [rawFormat] : rawFormat;

      const uri = resolveInputPath();
      if (uri === undefined) {
        writeEmitter.fire("No input path found for " + exportArgs.inputPath + "\r\n");
        return;
      }

      for (const format of formats) {
        const provider = formatProvider[format];
        if (!provider) {
          writeEmitter.fire("Unsupported export format: " + format + "\r\n");
          continue;
        }

        const extraOpts = provider.opts();
        writeEmitter.fire("Exporting to " + format + "... " + obj(extraOpts) + "\r\n");
        const outputPath = await provider.export(uri, extraOpts);
        writeEmitter.fire("Exported to " + outputPath + "\r\n");
      }
    }

    function resolveInputPath() {
      const inputPath = exportArgs.inputPath;
      if (inputPath === "$focused" || inputPath === undefined) {
        return getFocusingFile();
      }

      return inputPath;
    }

    function resolvePageOpts(): any {
      if (exportArgs.merged) {
        return {
          merged: {
            gap: exportArgs["merged.gap"],
          },
        };
      }
      return "first";
    }
  });

  function obj(obj: any): string {
    return JSON.stringify(obj, null, 1).replace(/\n/g, "\r\n");
  }
}
