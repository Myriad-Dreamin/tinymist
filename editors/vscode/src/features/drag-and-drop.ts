import * as vscode from "vscode";
import { dirname, extname, relative } from "path";
import { typstDocumentSelector } from "../util";

export function dragAndDropActivate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.languages.registerDocumentDropEditProvider(typstDocumentSelector, new TextProvider()),
  );
}

enum ResourceKind {
  BuiltinImage,
  Webp,
  Source,
  Markdown,
  TeX,
  Json,
  Toml,
  Csv,
  Yaml,
  Bib,
  Xlsx,
}

const resourceKinds: Record<string, ResourceKind> = {
  ".jpg": ResourceKind.BuiltinImage,
  ".jpeg": ResourceKind.BuiltinImage,
  ".png": ResourceKind.BuiltinImage,
  ".gif": ResourceKind.BuiltinImage,
  ".bmp": ResourceKind.BuiltinImage,
  ".ico": ResourceKind.BuiltinImage,
  ".svg": ResourceKind.BuiltinImage,
  ".webp": ResourceKind.Webp,
  ".typst": ResourceKind.Source,
  ".typ": ResourceKind.Source,
  ".md": ResourceKind.Markdown,
  ".tex": ResourceKind.TeX,
  ".json": ResourceKind.Json,
  ".jsonc": ResourceKind.Json,
  ".json5": ResourceKind.Json,
  ".toml": ResourceKind.Toml,
  ".csv": ResourceKind.Csv,
  ".yaml": ResourceKind.Yaml,
  ".yml": ResourceKind.Yaml,
  ".bib": ResourceKind.Bib,
  ".xlsx": ResourceKind.Xlsx,
};

export class TextProvider implements vscode.DocumentDropEditProvider {
  async provideDocumentDropEdits(
    doc: vscode.TextDocument,
    position: vscode.Position,
    dataTransfer: vscode.DataTransfer,
    token: vscode.CancellationToken,
  ): Promise<vscode.DocumentDropEdit | undefined> {
    const plainText = dataTransfer.get("text/plain");
    if (!plainText) {
      return;
    }

    const dropFileUri = doc.uri;
    const dragFileUri = vscode.Uri.parse(plainText.value);

    let dragFilePath = "";
    let workspaceFolder = vscode.workspace.getWorkspaceFolder(dragFileUri);
    if (dropFileUri.scheme === "untitled") {
      if (workspaceFolder) {
        dragFilePath = relative(workspaceFolder.uri.fsPath, dragFileUri.fsPath);
      }
    } else {
      dragFilePath = relative(dirname(dropFileUri.fsPath), dragFileUri.fsPath);
    }

    let barPath = dragFilePath.replace(/\\/g, "/");
    let strPath = `"${barPath}"`;
    let codeSnippet = strPath;
    let resourceKind: ResourceKind | undefined = resourceKinds[extname(dragFileUri.fsPath)];
    // todo: fetch latest version
    const additionalPkgs: [string, string, string | undefined][] = [];
    switch (resourceKind) {
      case ResourceKind.BuiltinImage:
        codeSnippet = `image(${strPath})`;
        break;
      case ResourceKind.Webp:
        additionalPkgs.push(["@preview/grayness", "0.1.0", "grayscale-image"]);
        codeSnippet = `grayscale-image(read(${strPath}))`;
        break;
      case ResourceKind.Webp:
        additionalPkgs.push(["@preview/rexllent", "0.2.0", "xlsx-parser"]);
        codeSnippet = `xlsx-parser(read(${strPath}, encoding: none)`;
        break;
      case ResourceKind.Source:
        codeSnippet = `include ${strPath}`;
        break;
      case ResourceKind.Markdown:
        additionalPkgs.push(["@preview/cmarker", "0.1.1", undefined]);
        codeSnippet = `cmarker.render(read(${strPath}))`;
        break;
      case ResourceKind.TeX:
        additionalPkgs.push(["@preview/mitex", "0.2.4", "mitex"]);
        codeSnippet = `mitex(read(${strPath}))`;
        break;
      case ResourceKind.Json:
        codeSnippet = `json(${strPath})`;
        break;
      case ResourceKind.Toml:
        codeSnippet = `toml(${strPath})`;
        break;
      case ResourceKind.Csv:
        codeSnippet = `csv(${strPath})`;
        break;
      case ResourceKind.Yaml:
        codeSnippet = `yaml(${strPath})`;
        break;
      case ResourceKind.Bib:
        codeSnippet = `bibliography(${strPath})`;
        break;
      default:
        codeSnippet = `read(${strPath})`;
        break;
    }

    const res = await vscode.commands.executeCommand<
      [{ mode: "math" | "markup" | "code" | "comment" | "string" | "raw" }]
    >("tinymist.interactCodeContext", {
      textDocument: {
        uri: doc.uri.toString(),
      },
      query: [
        {
          kind: "modeAt",
          position: {
            line: position.line,
            character: position.character,
          },
        },
      ],
    });

    let text = codeSnippet;
    switch (res?.[0]?.mode) {
      case "math":
      case "markup":
        text = `#${codeSnippet}`;
        break;
      case "code":
        text = codeSnippet;
        break;
      case "comment":
      case "raw":
      case "string":
        text = barPath;
        break;
    }

    let additionalEdit = undefined;
    if (additionalPkgs.length > 0) {
      additionalEdit = new vscode.WorkspaceEdit();
      const t = doc.getText();
      for (const [pkgName, version, importName] of additionalPkgs) {
        if (!t.includes(pkgName)) {
          if (importName) {
            additionalEdit.insert(
              doc.uri,
              new vscode.Position(0, 0),
              `#import "${pkgName}:${version}": ${importName}\n`,
            );
          } else {
            additionalEdit.insert(
              doc.uri,
              new vscode.Position(0, 0),
              `#import "${pkgName}:${version}"\n`,
            );
          }
        }
      }
    }

    // console.log(resourceKind, res?.[0]?.mode, codeSnippet, text);

    const insertText = new vscode.SnippetString(text);
    const edit = new vscode.DocumentDropEdit(insertText);
    edit.additionalEdit = additionalEdit;

    return edit;
  }
}
