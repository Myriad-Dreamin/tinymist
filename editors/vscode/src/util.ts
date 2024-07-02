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

export class DisposeList {
    disposes: (() => void)[] = [];
    disposed = false;
    constructor() {}
    add(d: (() => void) | vscode.Disposable) {
        if (this.disposed) {
            // console.error("disposed", this.taskId, "for", this.filePath);
            return;
        }

        if (typeof d === "function") {
            this.disposes.push(d);
        } else {
            this.disposes.push(() => d.dispose());
        }
    }
    dispose() {
        if (this.disposed) {
            return;
        }
        this.disposed = true;

        for (const d of this.disposes) {
            d();
        }
    }
}
