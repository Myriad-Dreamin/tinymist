import * as vscode from "vscode";
import * as path from "path";
import { ViewColumn } from "vscode";
import { readFile } from "fs/promises";

export function getTargetViewColumn(viewColumn: ViewColumn | undefined): ViewColumn {
    if (viewColumn === ViewColumn.One) {
        return ViewColumn.Two;
    }
    if (viewColumn === ViewColumn.Two) {
        return ViewColumn.One;
    }
    return ViewColumn.Beside;
}

export async function loadHTMLFile(context: vscode.ExtensionContext, relativePath: string) {
    const filePath = path.resolve(context.extensionPath, relativePath);
    const fileContents = await readFile(filePath, "utf8");
    return fileContents;
}
