import { ViewColumn } from "vscode";

export function getTargetViewColumn(viewColumn: ViewColumn | undefined): ViewColumn {
  if (viewColumn === ViewColumn.One) {
    return ViewColumn.Two;
  }
  if (viewColumn === ViewColumn.Two) {
    return ViewColumn.One;
  }
  return ViewColumn.Beside;
}
