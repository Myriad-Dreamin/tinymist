import * as vscode from "vscode";
import * as fs from "node:fs";
import * as path from "node:path";
import { l10nMsg } from "./l10n";
import { tinymist } from "./lsp";

/// The online installation guide for this extension.
const INSTALL_DOC_URL = "https://myriad-dreamin.github.io/tinymist/frontend/vscode.html";

/// Why probing for the tinymist binary failed.
export type ProbeFailureKind =
  /// The user-configured `tinymist.serverPath` does not point to a working binary.
  | "server-path"
  /// The installed package ships without a bundled binary (the universal marketplace package).
  | "universal-no-bundled"
  /// A bundled binary exists, but it could not be executed (e.g. quarantined by antivirus software).
  | "bundled-unexecutable";

/// Release assets for the platform running the extension.
export interface PlatformPackageInfo {
  /// The VSIX asset on GitHub Releases that bundles a binary for this platform, if any.
  vsix: string | undefined;
  /// The CLI archive asset on GitHub Releases for this platform.
  cli: string;
  /// True when this platform is musl-based (e.g. Alpine Linux), where the glibc binaries bundled
  /// in the linux VSIX packages do not work.
  musl: boolean;
}

/// Resolves the path of the binary bundled inside the installed package.
export function bundledBinaryPath(context: vscode.ExtensionContext): string {
  const binaryName = process.platform === "win32" ? "tinymist.exe" : "tinymist";
  return path.resolve(context.extensionPath, "out", binaryName);
}

/// Classifies why probing for the tinymist binary failed.
export function classifyProbeFailure(
  context: vscode.ExtensionContext,
  configPath: string | undefined,
): ProbeFailureKind {
  if (configPath) {
    return "server-path";
  }
  return fs.existsSync(bundledBinaryPath(context))
    ? "bundled-unexecutable"
    : "universal-no-bundled";
}

function isMuslPlatform(): boolean {
  if (process.platform !== "linux") {
    return false;
  }
  // Node.js exposes the glibc version in its process report when running on glibc.
  // On musl-based systems (e.g. Alpine Linux) the field is absent.
  try {
    const report = process.report?.getReport?.() as
      | { header?: { glibcVersion?: string } }
      | undefined;
    return report?.header?.glibcVersion === undefined;
  } catch {
    return false;
  }
}

/// Maps the running platform to the release assets that match it.
export function platformPackageInfo(
  platform: string = process.platform,
  arch: string = process.arch,
  musl: boolean = isMuslPlatform(),
): PlatformPackageInfo {
  if (platform === "win32") {
    return {
      vsix: arch === "arm64" ? "tinymist-win32-arm64.vsix" : "tinymist-win32-x64.vsix",
      cli:
        arch === "arm64"
          ? "tinymist-aarch64-pc-windows-msvc.zip"
          : "tinymist-x86_64-pc-windows-msvc.zip",
      musl: false,
    };
  }
  if (platform === "darwin") {
    return {
      vsix: arch === "arm64" ? "tinymist-darwin-arm64.vsix" : "tinymist-darwin-x64.vsix",
      cli:
        arch === "arm64"
          ? "tinymist-aarch64-apple-darwin.tar.gz"
          : "tinymist-x86_64-apple-darwin.tar.gz",
      musl: false,
    };
  }
  const arm64 = arch === "arm64";
  const cli = arm64
    ? musl
      ? "tinymist-aarch64-unknown-linux-musl.tar.gz"
      : "tinymist-aarch64-unknown-linux-gnu.tar.gz"
    : musl
      ? "tinymist-x86_64-unknown-linux-musl.tar.gz"
      : "tinymist-x86_64-unknown-linux-gnu.tar.gz";
  if (musl) {
    return {
      // Only the x64 Alpine VSIX exists; the binary bundled in the linux-arm64 VSIX is a glibc
      // binary and does not work on musl systems.
      vsix: arm64 ? undefined : "tinymist-alpine-x64.vsix",
      cli,
      musl: true,
    };
  }
  const vsix = arm64
    ? "tinymist-linux-arm64.vsix"
    : arch === "arm"
      ? "tinymist-linux-armhf.vsix"
      : "tinymist-linux-x64.vsix";
  return { vsix, cli, musl };
}

/// The one-line human message for a probe failure, with the key diagnostic hint.
export function buildProbeFailureMessage(
  kind: ProbeFailureKind,
  info: PlatformPackageInfo,
  platform: string = process.platform,
): string {
  const unavailable =
    " Some features that rely on the language server (completion, hover, previews) will not work until the binary is available.";
  switch (kind) {
    case "server-path":
      return (
        l10nMsg("The tinymist binary configured in tinymist.serverPath could not be started.") +
        unavailable
      );
    case "universal-no-bundled":
      return (
        (info.musl
          ? "This VS Code package does not bundle a binary for musl-based systems (Alpine Linux)."
          : "This VS Code package does not bundle the tinymist binary.") +
        " " +
        (info.musl
          ? `Install the tinymist CLI yourself, for example the ${info.cli} archive from GitHub Releases, and point tinymist.serverPath to it.`
          : "Install the tinymist CLI yourself (a GitHub Releases archive or cargo install tinymist), or switch to a platform-specific marketplace package, then point tinymist.serverPath to it if needed.") +
        unavailable
      );
    case "bundled-unexecutable":
      if (platform === "win32") {
        return (
          "The bundled tinymist binary could not be executed. If a freshly installed binary keeps being reported missing, it is usually quarantined by antivirus software: restore it from the quarantine, add it to the allow list, and restart VS Code." +
          unavailable
        );
      }
      return (
        "The bundled tinymist binary could not be executed. Check the file permissions, or reinstall the extension." +
        unavailable
      );
  }
}

/// The docs site anchors headings as `label-<heading text>`; the universal-package failure jumps
/// directly to the self-installation subsection.
export function installDocUrl(kind: ProbeFailureKind): string {
  const anchor =
    kind === "universal-no-bundled"
      ? "label-Install the CLI yourself"
      : "label-Installation & Troubleshooting";
  return `${INSTALL_DOC_URL}#${encodeURIComponent(anchor)}`;
}

/// Shows the installation guide notification for a failed binary probe: classifies the failure,
/// logs the per-candidate probe diagnostics, and offers actions instead of a dead-end error.
export async function showInstallGuideOnProbeFailure(
  context: vscode.ExtensionContext,
  configName: string,
  configPath: string | undefined,
  error: unknown,
): Promise<void> {
  const kind = classifyProbeFailure(context, configPath);
  const info = platformPackageInfo();

  const output = tinymist.outputChannel;
  const detail = error instanceof Error ? error.message : `${error}`;
  output.appendLine(`Failed to find a valid tinymist binary.`);
  output.appendLine(`- platform: ${process.platform}-${process.arch}`);
  output.appendLine(`- extension directory: ${path.basename(context.extensionPath)}`);
  output.appendLine(`- failure kind: ${kind}`);
  output.appendLine(`- probe detail:\n${detail}`);

  const openGuide = l10nMsg("Open installation guide");
  const configurePath = l10nMsg("Configure tinymist.serverPath");
  const dismiss = l10nMsg("Not now");
  const selection = await vscode.window.showErrorMessage(
    buildProbeFailureMessage(kind, info),
    openGuide,
    configurePath,
    dismiss,
  );
  if (selection === openGuide) {
    void vscode.env.openExternal(vscode.Uri.parse(installDocUrl(kind)));
  } else if (selection === configurePath) {
    void vscode.commands.executeCommand("workbench.action.openSettings", configName);
  }
}
