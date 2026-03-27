import { afterEach, beforeEach, expect, test, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  getConfiguration: vi.fn(),
  showErrorMessage: vi.fn(),
  vscodeVariables: vi.fn((value: string) => value.replace(/\$\{workspaceFolder\}/g, "/workspace")),
}));

vi.mock("vscode", () => ({
  workspace: {
    getConfiguration: hoisted.getConfiguration,
  },
  window: {
    activeColorTheme: { kind: 1 },
    showErrorMessage: hoisted.showErrorMessage,
  },
  ColorThemeKind: {
    Dark: 2,
    HighContrast: 3,
  },
}));

vi.mock("./vscode-variables", () => ({
  vscodeVariables: hoisted.vscodeVariables,
}));

import { loadTinymistConfig, substVscodeVarsInConfig } from "./config.js";

beforeEach(() => {
  hoisted.getConfiguration.mockReset();
  hoisted.showErrorMessage.mockReset();
  hoisted.vscodeVariables.mockClear();
  vi.spyOn(console, "warn").mockImplementation(() => undefined);
});

afterEach(() => {
  vi.restoreAllMocks();
});

test("loadTinymistConfig preserves substitution for valid fontPaths arrays", () => {
  hoisted.getConfiguration.mockReturnValue({
    fontPaths: ["${workspaceFolder}/fonts", "assets/fonts"],
  });

  expect(loadTinymistConfig().fontPaths).toEqual(["/workspace/fonts", "assets/fonts"]);
  expect(substVscodeVarsInConfig(["tinymist.fontPaths"], [["${workspaceFolder}/fonts"]])).toEqual([
    ["/workspace/fonts"],
  ]);
  expect(hoisted.showErrorMessage).not.toHaveBeenCalled();
});

test("substVscodeVarsInConfig ignores mixed-type fontPaths arrays", () => {
  expect(
    substVscodeVarsInConfig(["tinymist.fontPaths"], [["${workspaceFolder}/fonts", 1]]),
  ).toEqual([undefined]);
  expect(hoisted.showErrorMessage).toHaveBeenCalledTimes(1);
});
