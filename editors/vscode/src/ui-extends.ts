import * as vscode from "vscode";
import { statusBarFormatString } from "./extension";

let statusBarItem: vscode.StatusBarItem;

function initWordCountItem() {
  statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 1);
  statusBarItem.name = "Tinymist Status";
  statusBarItem.command = "tinymist.showLog";
  return statusBarItem;
}

let words = 0;
let chars = 0;
let spaces = 0;
let cjkChars = 0;

interface WordsCount {
  words: number;
  chars: number;
  spaces: number;
  cjkChars: number;
}

export interface TinymistStatus {
  status: "compiling" | "compileSuccess" | "compileError";
  path: string;
  wordsCount: WordsCount;
}

export const triggerStatusBar = (show: boolean) => {
  statusBarItem = statusBarItem || initWordCountItem();
  if (show) {
    statusBarItem.show();
  } else {
    statusBarItem.hide();
  }
};

export function wordCountItemProcess(event: TinymistStatus) {
  statusBarItem = statusBarItem || initWordCountItem();

  const updateTooltip = () => {
    statusBarItem.tooltip = `
Main file: ${event.path}
${words} ${plural("Word", words)}
${chars} ${plural("Character", chars)}
${spaces} ${plural("Space", spaces)}
${cjkChars} CJK ${plural("Character", cjkChars)}
[Click to show logs]`.trim();
  };

  words = event.wordsCount?.words || 0;
  chars = event.wordsCount?.chars || 0;
  spaces = event.wordsCount?.spaces || 0;
  cjkChars = event.wordsCount?.cjkChars || 0;

  const fileName = event.path ? event.path.split("/").slice(-1)[0] : "";
  const fileNameWithoutExt = fileName ? fileName.split(".").slice(0, -1).join(".") : "";

  const formatString = statusBarFormatString()
    .replace(/\{wordCount\}/g, `${words} ${plural("Word", words)}`)
    .replace(/\{fileName\}/g, fileNameWithoutExt);

  if (statusBarItem) {
    if (event.status === "compiling") {
      statusBarItem.text = formatString.replace(/\{compileStatusIcon\}/g, "$(sync~spin)");
      statusBarItem.backgroundColor = new vscode.ThemeColor("statusBarItem.prominentBackground");
      updateTooltip();
    } else if (event.status === "compileSuccess") {
      statusBarItem.text = formatString.replace(/\{compileStatusIcon\}/g, "$(sync)");
      statusBarItem.backgroundColor = new vscode.ThemeColor("statusBarItem.prominentBackground");
      updateTooltip();
    } else if (event.status === "compileError") {
      statusBarItem.text = formatString.replace(/\{compileStatusIcon\}/g, "$(sync)");
      statusBarItem.backgroundColor = new vscode.ThemeColor("statusBarItem.errorBackground");
      updateTooltip();
    }
  }
}
function plural(w: string, words: number): string {
  if (words == 1) {
    return w;
  } else {
    return w + "s";
  }
}
