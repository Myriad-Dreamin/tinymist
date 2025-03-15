import * as vscode from "vscode";
import { IContext } from "../context";
import { testingCovFeatureActivate } from "./testing/coverage";

export function testingFeatureActivate(context: IContext) {
  const testController = vscode.tests.createTestController(
    "tinymist-tests",
    "Typst Tests (Tinymist)",
  );

  testingCovFeatureActivate(context, testController);
}
