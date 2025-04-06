import * as vscode from "vscode";
import { IContext } from "../context";
import { testingCovActivate } from "./testing/coverage";

export function testingActivate(context: IContext) {
  const testController = vscode.tests.createTestController(
    "tinymist-tests",
    "Typst Tests (Tinymist)",
  );

  testingCovActivate(context, testController);
}
