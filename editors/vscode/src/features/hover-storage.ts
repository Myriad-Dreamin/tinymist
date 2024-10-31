import * as vscode from "vscode";
import * as crypto from "crypto";
import { Uri } from "vscode";
import { base64Decode } from "../util";

export interface HoverStorage {
  startHover(): Promise<HoverStorageHandler>;
}

export interface HoverStorageHandler {
  baseUri(): vscode.Uri | undefined;
  storeImage(image: string): string;
  finish(): Promise<void>;
}

export class HoverDummyStorage {
  startHover() {
    return Promise.resolve(new HoverStorageDummyHandler());
  }
}

export class HoverTmpStorage {
  constructor(readonly context: vscode.ExtensionContext) {}

  async startHover() {
    try {
      // This is a "workspace wide" storage for temporary hover images
      if (this.context.storageUri) {
        const tmpImageDir = Uri.joinPath(this.context.storageUri, "tmp/hover-images/");
        const previousEntries = await vscode.workspace.fs.readDirectory(tmpImageDir);
        let deleted = 0;
        for (const [name, type] of previousEntries) {
          if (type === vscode.FileType.File) {
            deleted++;
            await vscode.workspace.fs.delete(Uri.joinPath(tmpImageDir, name));
          }
        }
        if (deleted > 0) {
          console.log(`Deleted ${deleted} hover images`);
        }

        return new HoverStorageTmpFsHandler(Uri.joinPath(this.context.storageUri, "tmp/"));
      }
    } catch {}

    return new HoverStorageDummyHandler();
  }
}

class HoverStorageTmpFsHandler {
  promises: PromiseLike<void>[] = [];

  constructor(readonly _baseUri: vscode.Uri) {}

  baseUri() {
    return this._baseUri;
  }

  storeImage(content: string) {
    const fs = vscode.workspace.fs;
    const hash = crypto.createHash("sha256").update(content).digest("hex");
    const tmpImagePath = `./hover-images/${hash}.svg`;
    const output = Uri.joinPath(this._baseUri, tmpImagePath);
    const outputContent = base64Decode(content);
    this.promises.push(fs.writeFile(output, Buffer.from(outputContent, "utf-8")));
    return tmpImagePath;
  }

  async finish() {
    await Promise.all(this.promises);
  }
}

class HoverStorageDummyHandler {
  baseUri() {
    return undefined;
  }

  storeImage(_content: string) {
    return "";
  }

  async finish() {
    return;
  }
}
