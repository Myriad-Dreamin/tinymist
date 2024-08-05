import * as vscode from "vscode";
import { runExport } from "./tasks.export";

export const TYPST_TASK_SOURCE = "typst";

export function taskActivate(context: vscode.ExtensionContext) {
  const provide = (cls: typeof TypstTaskProvider) =>
    context.subscriptions.push(vscode.tasks.registerTaskProvider(cls.TYPE, new cls(context)));

  provide(TypstTaskProvider);
}

class TypstTaskProvider implements vscode.TaskProvider {
  static readonly TYPE = "typst";

  static commands = {
    export: {
      runner: runExport,
      group: vscode.TaskGroup.Build,
    },
  } as const;

  constructor(private readonly context: vscode.ExtensionContext) {}

  static has(task: vscode.Task): boolean {
    return task.definition.type === TypstTaskProvider.TYPE;
  }

  async provideTasks(): Promise<vscode.Task[]> {
    return [];
  }

  async resolveTask(task: vscode.Task): Promise<vscode.Task | undefined> {
    if (!TypstTaskProvider.has(task)) {
      return task;
    }

    for (const [command, { runner, group }] of Object.entries(TypstTaskProvider.commands)) {
      if (task.definition.command !== command) {
        continue;
      }
      const resolved = new vscode.Task(
        task.definition,
        task.scope || vscode.TaskScope.Workspace,
        task.name,
        TYPST_TASK_SOURCE,
        new vscode.CustomExecution(runner),
      );
      resolved.group = group;
      return resolved;
    }

    return task;
  }
}
