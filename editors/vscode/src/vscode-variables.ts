//! Upstream https://github.com/DominicVonk/vscode-variables/blob/ff54d17a8abab9a5735365262ae471003ea9015a/index.js
//! Last Checked: 2024-07-25
//! Since it is not well maintained, we copy the source code here for further development.

import * as vscode from "vscode";
import * as process from "process";
import * as path from "path";

import { extensionState } from "./state";

export function vscodeVariables(
  string: string,
  recursive?: boolean,
  context = new CodeVariableContext(),
): string {
  while (true) {
    string = string.replace(context.regex, (match) => {
      let key = match.slice(2, -1);
      // trim the variable name
      if (key.includes(":")) {
        key = key.slice(0, key.indexOf(":"));
      }
      return context.replacers[key]?.value(match) || "";
    });

    if (!recursive) {
      return string;
    }

    const anyProgress = string.match(context.regex);
    if (!anyProgress) {
      return string;
    }
  }
}

export class CodeVariableContext {
  replacers: Record<string, Replacer> = {
    workspaceFolder: { value: () => this.workspace?.uri.fsPath || "" },
    // is this correct? (this.workspace?.name does not always equal to the folder name)
    workspaceFolderBasename: { value: () => this.workspace?.name || "" },
    file: { value: () => this.absoluteFilePath || "" },
    fileWorkspaceFolder: {
      value: () => this.workspaceVars.activeWorkspace?.uri.fsPath || "",
    },
    relativeFile: { value: () => this.workspaceVars.relativeFilePath || "" },
    relativeFileDirname: { value: () => this.workspaceVars.relativeFileDirname || "" },
    fileBasename: { value: () => this.parsedPath.base },
    fileBasenameNoExtension: { value: () => this.parsedPath.name },
    fileExtname: { value: () => this.parsedPath.ext },
    fileDirname: {
      value: () => this.parsedPath.dir.substr(this.parsedPath.dir.lastIndexOf(path.sep) + 1),
    },
    cwd: { value: () => this.parsedPath.dir },
    pathSeparator: { value: () => path.sep },
    lineNumber: { value: () => this.lineNumber.toString() },
    selectedText: { value: () => this.selectedText },
    env: {
      variable: true,
      value: (variable: string) => {
        if (extensionState.features.web) {
          return "";
        }

        const e = variable.match(/\${env:(.*?)}/);
        return (e && process.env?.[e[1]]) || "";
      },
    },
    config: {
      variable: true,
      value: (variable: string) => {
        const e = variable.match(/\${config:(.*?)}/);
        return (e && vscode.workspace.getConfiguration().get(e[1], "")) || "";
      },
    },
  };

  static variableRegexCache: RegExp | undefined;
  get regex() {
    return (
      CodeVariableContext.variableRegexCache ||
      (CodeVariableContext.variableRegexCache = variableRegex(this.replacers))
    );
  }

  private workspaces?: readonly vscode.WorkspaceFolder[];
  workspace?: vscode.WorkspaceFolder;
  private activeTextEditor?: vscode.TextEditor;

  constructor(public code = vscode) {
    this.workspaces = code?.workspace.workspaceFolders;
    this.workspace = this.workspaces?.length ? this.workspaces[0] : undefined;
    this.activeTextEditor = code?.window.activeTextEditor;
    this.absoluteFilePath = this.activeTextEditor?.document.uri.fsPath;
    this.lineNumber = (this.activeTextEditor?.selection.start.line || 0) + 1;
  }

  static test(vars: {
    absoluteFilePath?: string;
    activeWorkspace?: vscode.WorkspaceFolder;
    relativeFilePath?: string;
    parsedPath?: path.ParsedPath;
    lineNumber?: number;
    selectedText?: string;
  }) {
    const context = new CodeVariableContext();
    context.workspaceVariableCache = vars;
    context.parsedPathCache = vars.parsedPath;
    context.absoluteFilePath = vars.absoluteFilePath;
    context.lineNumber = vars.lineNumber || 1;
    context.selectedTextCache = { value: vars.selectedText || "" };
    return context;
  }

  absoluteFilePath?: string;

  private workspaceVariableCache?: {
    activeWorkspace?: vscode.WorkspaceFolder;
    relativeFilePath?: string;
    relativeFileDirname?: string;
  };
  get workspaceVars() {
    if (this.workspaceVariableCache) {
      return this.workspaceVariableCache;
    }

    let activeWorkspace = this.workspace;
    let relativeFilePath = this.absoluteFilePath;
    let relativeFileDirname = undefined;
    if (this.workspaces && this.absoluteFilePath) {
      for (const workspace of this.workspaces) {
        if (this.absoluteFilePath.replace(workspace.uri.fsPath, "") !== this.absoluteFilePath) {
          activeWorkspace = workspace;
          relativeFilePath = this.absoluteFilePath
            .replace(workspace.uri.fsPath, "")
            .substr(path.sep.length);
          break;
        }
      }
      relativeFileDirname = relativeFilePath?.substr(0, relativeFilePath?.lastIndexOf(path.sep));
    }

    return (this.workspaceVariableCache = {
      activeWorkspace,
      relativeFilePath,
      relativeFileDirname,
    });
  }

  private parsedPathCache?: path.ParsedPath;
  get parsedPath() {
    if (this.parsedPathCache) {
      return this.parsedPathCache;
    }

    return (this.parsedPathCache = path.parse(this.absoluteFilePath || ""));
  }

  lineNumber: number;

  private selectedTextCache?: { value: string };
  get selectedText() {
    if (this.selectedTextCache) {
      return this.selectedTextCache.value;
    }

    let selectedText = "";
    const activeTextEditor = this.activeTextEditor;
    if (activeTextEditor) {
      selectedText = activeTextEditor.document.getText(
        new vscode.Range(activeTextEditor.selection.start, activeTextEditor.selection.end),
      );
    }

    this.selectedTextCache = {
      value: selectedText,
    };

    return selectedText;
  }
}

function variableRegex(replacers: Record<string, Replacer>) {
  const regexParts = [];
  regexParts.push("\\${(");
  for (const key in replacers) {
    regexParts.push("|");
    regexParts.push(key);
    if (replacers[key].variable) {
      regexParts.push(`:.*?`);
    }
  }
  regexParts.push(")}");
  return new RegExp(regexParts.join(""), "g");
}
interface PureReplacer {
  variable?: false;
  value: () => string;
}
interface VarReplacer {
  variable: true;
  value: (variable: string) => string;
}
type Replacer = PureReplacer | VarReplacer;
