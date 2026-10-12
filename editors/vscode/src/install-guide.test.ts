import { beforeEach, expect, test, vi } from "vitest";
import * as path from "node:path";

const hoisted = vi.hoisted(() => ({
  appendLine: vi.fn(),
  showErrorMessage: vi.fn(),
  openExternal: vi.fn(),
  executeCommand: vi.fn(),
  existsSync: vi.fn(),
}));

vi.mock("vscode", () => ({
  window: {
    showErrorMessage: hoisted.showErrorMessage,
  },
  env: {
    openExternal: hoisted.openExternal,
  },
  commands: {
    executeCommand: hoisted.executeCommand,
  },
  Uri: {
    parse: (url: string) => url,
  },
  l10n: {
    t: (message: string) => message,
  },
}));

vi.mock("node:fs", () => ({
  existsSync: hoisted.existsSync,
}));

vi.mock("./lsp", () => ({
  tinymist: {
    outputChannel: {
      appendLine: hoisted.appendLine,
    },
  },
}));

import {
  bundledBinaryPath,
  buildProbeFailureMessage,
  classifyProbeFailure,
  installDocUrl,
  platformPackageInfo,
  showInstallGuideOnProbeFailure,
} from "./install-guide";

const context = { extensionPath: "/extensions/tinymist-0.15.10-win32-x64" } as never;

beforeEach(() => {
  vi.clearAllMocks();
});

test("bundledBinaryPath resolves the packaged binary with platform suffix", () => {
  const binarySuffix = process.platform === "win32" ? "tinymist.exe" : "tinymist";
  expect(bundledBinaryPath(context)).toBe(
    path.join(path.resolve(context.extensionPath), "out", binarySuffix),
  );
});

test("classifyProbeFailure distinguishes configured path, missing and unexecutable bundles", () => {
  expect(classifyProbeFailure(context, "/custom/tinymist")).toBe("server-path");
  hoisted.existsSync.mockReturnValueOnce(false);
  expect(classifyProbeFailure(context, undefined)).toBe("universal-no-bundled");
  hoisted.existsSync.mockReturnValueOnce(true);
  expect(classifyProbeFailure(context, undefined)).toBe("bundled-unexecutable");
});

test("platformPackageInfo maps release assets per platform and architecture", () => {
  expect(platformPackageInfo("win32", "x64", false)).toEqual({
    vsix: "tinymist-win32-x64.vsix",
    cli: "tinymist-x86_64-pc-windows-msvc.zip",
    musl: false,
  });
  expect(platformPackageInfo("win32", "arm64", false)).toEqual({
    vsix: "tinymist-win32-arm64.vsix",
    cli: "tinymist-aarch64-pc-windows-msvc.zip",
    musl: false,
  });
  expect(platformPackageInfo("darwin", "x64", false)).toEqual({
    vsix: "tinymist-darwin-x64.vsix",
    cli: "tinymist-x86_64-apple-darwin.tar.gz",
    musl: false,
  });
  expect(platformPackageInfo("darwin", "arm64", false)).toEqual({
    vsix: "tinymist-darwin-arm64.vsix",
    cli: "tinymist-aarch64-apple-darwin.tar.gz",
    musl: false,
  });
  expect(platformPackageInfo("linux", "x64", false)).toEqual({
    vsix: "tinymist-linux-x64.vsix",
    cli: "tinymist-x86_64-unknown-linux-gnu.tar.gz",
    musl: false,
  });
  expect(platformPackageInfo("linux", "arm64", true)).toEqual({
    // The linux-arm64 VSIX bundles a glibc binary; musl systems must use the CLI archive.
    vsix: undefined,
    cli: "tinymist-aarch64-unknown-linux-musl.tar.gz",
    musl: true,
  });
  expect(platformPackageInfo("linux", "x64", true)).toEqual({
    vsix: "tinymist-alpine-x64.vsix",
    cli: "tinymist-x86_64-unknown-linux-musl.tar.gz",
    musl: true,
  });
});

test("buildProbeFailureMessage mentions antivirus quarantine on Windows unexecutable bundles", () => {
  const winMsg = buildProbeFailureMessage(
    "bundled-unexecutable",
    platformPackageInfo("win32", "x64"),
    "win32",
  );
  expect(winMsg).toContain("quarantined by antivirus software");
  const unixMsg = buildProbeFailureMessage(
    "bundled-unexecutable",
    platformPackageInfo("linux", "x64"),
    "linux",
  );
  expect(unixMsg).not.toContain("antivirus");
  expect(unixMsg).toContain("could not be executed");

  const serverPathMsg = buildProbeFailureMessage(
    "server-path",
    platformPackageInfo("win32", "x64"),
    "win32",
  );
  expect(serverPathMsg).toContain("tinymist.serverPath");

  const muslMsg = buildProbeFailureMessage(
    "universal-no-bundled",
    platformPackageInfo("linux", "x64", true),
    "linux",
  );
  expect(muslMsg).toContain("musl");
  expect(muslMsg).toContain("tinymist-x86_64-unknown-linux-musl.tar.gz");
});

test("installDocUrl routes the universal-package failure to the self-installation section", () => {
  expect(installDocUrl("universal-no-bundled")).toBe(
    "https://myriad-dreamin.github.io/tinymist/frontend/vscode.html#label-Install%20the%20CLI%20yourself",
  );
  expect(installDocUrl("bundled-unexecutable")).toBe(
    "https://myriad-dreamin.github.io/tinymist/frontend/vscode.html#label-Installation%20%26%20Troubleshooting",
  );
});

test("showInstallGuideOnProbeFailure logs diagnostics and routes the guide action", async () => {
  hoisted.existsSync.mockReturnValue(false);
  hoisted.showErrorMessage.mockResolvedValue("Open installation guide");

  await showInstallGuideOnProbeFailure(
    context,
    "tinymist.serverPath",
    undefined,
    new Error("probe failed"),
  );

  const logged = hoisted.appendLine.mock.calls.map((c) => c.join(" ")).join("\n");
  expect(logged).toContain("Failed to find a valid tinymist binary.");
  expect(logged).toContain("platform:");
  expect(logged).toContain("tinymist-0.15.10-win32-x64");
  expect(logged).toContain("universal-no-bundled");
  expect(logged).toContain("probe failed");
  expect(hoisted.showErrorMessage).toHaveBeenCalledTimes(1);
  expect(hoisted.openExternal).toHaveBeenCalledTimes(1);
  expect(hoisted.openExternal).toHaveBeenCalledWith(
    "https://myriad-dreamin.github.io/tinymist/frontend/vscode.html#label-Install%20the%20CLI%20yourself",
  );
});

test("showInstallGuideOnProbeFailure routes the settings action and dismisses", async () => {
  hoisted.existsSync.mockReturnValue(true);
  hoisted.showErrorMessage.mockResolvedValue("Configure tinymist.serverPath");

  await showInstallGuideOnProbeFailure(
    context,
    "tinymist.serverPath",
    undefined,
    new Error("boom"),
  );

  expect(hoisted.openExternal).not.toHaveBeenCalled();
  expect(hoisted.executeCommand).toHaveBeenCalledWith(
    "workbench.action.openSettings",
    "tinymist.serverPath",
  );

  hoisted.showErrorMessage.mockClear();
  hoisted.executeCommand.mockClear();
  hoisted.openExternal.mockClear();
  hoisted.showErrorMessage.mockResolvedValue("Not now");
  await showInstallGuideOnProbeFailure(
    context,
    "tinymist.serverPath",
    undefined,
    new Error("boom"),
  );
  expect(hoisted.executeCommand).not.toHaveBeenCalled();
  expect(hoisted.openExternal).not.toHaveBeenCalled();
});
