import * as vscode from "vscode";

export interface HoverStorageProvider {
  new (context: vscode.ExtensionContext): HoverStorage;
}

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

export class HoverStorageDummyHandler {
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
