//! Upstream https://github.com/DominicVonk/vscode-variables/blob/ff54d17a8abab9a5735365262ae471003ea9015a/index.js
//! Last Checked: 2024-07-25
//! Since it is not well maintained, we copy the source code here for further development.

import vscode = require("vscode");
import process = require("process");
import path = require("path");
export function vscodeVariables(string: string, recursive?: boolean): string {
    let workspaces = vscode.workspace.workspaceFolders;
    let workspace = workspaces?.length ? workspaces[0] : null;

    let activeTextEditor = vscode.window.activeTextEditor;
    let absoluteFilePath = activeTextEditor?.document?.uri.fsPath;
    let parsedPath = path.parse(absoluteFilePath || "");

    let activeWorkspace = workspace;
    let relativeFilePath = absoluteFilePath;
    let relativeFileDirname = undefined;
    if (workspaces && absoluteFilePath) {
        for (let workspace of workspaces) {
            if (absoluteFilePath.replace(workspace.uri.fsPath, "") !== absoluteFilePath) {
                activeWorkspace = workspace;
                relativeFilePath = absoluteFilePath
                    .replace(workspace.uri.fsPath, "")
                    .substr(path.sep.length);
                break;
            }
        }
        relativeFileDirname = relativeFilePath?.substr(0, relativeFilePath?.lastIndexOf(path.sep));
    }
    let lineNumber = (activeTextEditor?.selection.start.line || 0) + 1;
    let selectedText = undefined;
    if (activeTextEditor) {
        selectedText = activeTextEditor?.document.getText(
            new vscode.Range(activeTextEditor?.selection.start, activeTextEditor?.selection.end)
        );
    }

    // todo: better performance
    while (true) {
        string = string
            .replace(/\${workspaceFolder}/g, workspace?.uri.fsPath || "")
            .replace(/\${workspaceFolderBasename}/g, workspace?.name || "")
            .replace(/\${file}/g, absoluteFilePath || "")
            .replace(/\${fileWorkspaceFolder}/g, activeWorkspace?.uri.fsPath || "")
            .replace(/\${relativeFile}/g, relativeFilePath || "")
            .replace(/\${relativeFileDirname}/g, relativeFileDirname || "")
            .replace(/\${fileBasename}/g, parsedPath.base)
            .replace(/\${fileBasenameNoExtension}/g, parsedPath.name)
            .replace(/\${fileExtname}/g, parsedPath.ext)
            .replace(
                /\${fileDirname}/g,
                parsedPath.dir.substr(parsedPath.dir.lastIndexOf(path.sep) + 1)
            )
            .replace(/\${cwd}/g, parsedPath.dir)
            .replace(/\${pathSeparator}/g, path.sep)
            .replace(/\${lineNumber}/g, lineNumber.toString())
            .replace(/\${selectedText}/g, selectedText || "")
            .replace(/\${env:(.*?)}/g, function (variable) {
                const e = variable.match(/\${env:(.*?)}/);
                return (e && process.env[e[1]]) || "";
            })
            .replace(/\${config:(.*?)}/g, function (variable) {
                const e = variable.match(/\${config:(.*?)}/);
                return (e && vscode.workspace.getConfiguration().get(e[1], "")) || "";
            });
        if (recursive) {
            const anyProgress = string.match(
                /\${(workspaceFolder|workspaceFolderBasename|fileWorkspaceFolder|relativeFile|fileBasename|fileBasenameNoExtension|fileExtname|fileDirname|cwd|pathSeparator|lineNumber|selectedText|env:(.*?)|config:(.*?))}/
            );
            if (anyProgress) {
                continue;
            }
        }

        return string;
    }
}
