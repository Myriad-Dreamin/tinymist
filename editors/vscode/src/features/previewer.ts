import { readFile } from "fs/promises";
import * as path from "path";
import * as vscode from "vscode";

import { resolvePreviewProviderValue } from "../config";
import { tinymist } from "../lsp";
import { loadHTMLFile } from "../util";

export type PreviewThemeSourceKind = "builtin" | "html" | "extension";

export interface PreviewThemeSourceMetadata {
  kind: PreviewThemeSourceKind;
  trusted: boolean;
  configuredProvider?: string;
  extensionId?: string;
  htmlPath?: string;
  compatibleTinymistVersion?: string;
  fallbackReason?: string;
}

export interface ResolvedPreviewTheme {
  html: string;
  htmlUri: vscode.Uri;
  localResourceRoots: vscode.Uri[];
  source: PreviewThemeSourceMetadata;
}

export interface TinymistPreviewTheme {
  htmlPath: string;
  compatibleTinymistVersion: string;
  isCompatible?(tinymistVersion: string): Promise<boolean> | boolean;
}

export interface TinymistPreviewThemeProvider {
  provideTheme(): Promise<TinymistPreviewTheme> | TinymistPreviewTheme;
}

interface PreviewThemeExtension {
  extensionUri: vscode.Uri;
  activate(): Thenable<unknown>;
}

export interface PreviewThemeResolverEnvironment {
  provider?: string;
  workspaceTrusted: boolean;
  tinymistVersion: string;
  builtinTheme: () => Promise<ResolvedPreviewTheme>;
  readHtmlFile?: (uri: vscode.Uri) => Promise<string>;
  getExtension?: (id: string) => PreviewThemeExtension | undefined;
  showWarning?: (message: string) => void;
}

const PREVIEW_THEME_CONTRACT_MARKERS = [
  "ws://127.0.0.1:23625",
  "preview-arg:previewMode:Doc",
  "preview-arg:state:",
] as const;

let builtinPreviewSourceMode: "compat" | "tinymist" = "compat";
let cachedPreviewTheme: { key: string; theme: ResolvedPreviewTheme } | undefined;
let lastWarningKey: string | undefined;

export function setPreviewBuiltinSourceMode(mode: "compat" | "tinymist") {
  if (builtinPreviewSourceMode === mode) {
    return;
  }
  builtinPreviewSourceMode = mode;
  invalidatePreviewThemeCache();
}

export function invalidatePreviewThemeCache() {
  cachedPreviewTheme = undefined;
  lastWarningKey = undefined;
}

export async function preloadPreviewTheme(context: vscode.ExtensionContext) {
  await resolvePreviewTheme(context);
}

export function getConfiguredPreviewProvider(): string | undefined {
  return resolvePreviewProviderValue(
    vscode.workspace.getConfiguration("tinymist").get<string>("preview.provider"),
  );
}

export function parsePreviewProvider(
  value: string | undefined,
):
  | { kind: "builtin" }
  | { kind: "html"; htmlPath: string }
  | { kind: "extension"; extensionId: string } {
  const provider = value?.trim();
  if (!provider) {
    return { kind: "builtin" };
  }

  if (provider.startsWith("html:")) {
    return {
      kind: "html",
      htmlPath: provider.slice("html:".length).trim(),
    };
  }

  return {
    kind: "extension",
    extensionId: provider,
  };
}

export async function resolveConfiguredPreviewTheme(
  environment: PreviewThemeResolverEnvironment,
): Promise<ResolvedPreviewTheme> {
  const provider = environment.provider?.trim();
  const parsed = parsePreviewProvider(provider);

  if (!environment.workspaceTrusted && parsed.kind !== "builtin") {
    return fallbackToBuiltin(environment, provider, "workspace is not trusted", false);
  }

  switch (parsed.kind) {
    case "builtin":
      return environment.builtinTheme();
    case "html":
      return resolveHtmlPreviewTheme(environment, provider, parsed.htmlPath);
    case "extension":
      return resolveExtensionPreviewTheme(environment, provider, parsed.extensionId);
  }
}

export async function resolvePreviewTheme(
  context: vscode.ExtensionContext,
): Promise<ResolvedPreviewTheme> {
  const provider = getConfiguredPreviewProvider();
  const cacheKey = JSON.stringify({
    builtinPreviewSourceMode,
    provider,
    trusted: vscode.workspace.isTrusted,
    version: context.extension.packageJSON.version,
  });
  if (cachedPreviewTheme?.key === cacheKey) {
    return cachedPreviewTheme.theme;
  }

  const theme = await resolveConfiguredPreviewTheme({
    provider,
    workspaceTrusted: vscode.workspace.isTrusted,
    tinymistVersion: String(context.extension.packageJSON.version),
    builtinTheme: () => resolveBuiltinPreviewTheme(context),
    showWarning: (message) => {
      const warningKey = `${cacheKey}:${message}`;
      if (lastWarningKey === warningKey) {
        return;
      }
      lastWarningKey = warningKey;
      console.warn(message);
      void vscode.window.showWarningMessage(message);
    },
    getExtension: (extensionId) => vscode.extensions.getExtension(extensionId),
  });

  cachedPreviewTheme = { key: cacheKey, theme };
  return theme;
}

async function resolveBuiltinPreviewTheme(
  context: vscode.ExtensionContext,
): Promise<ResolvedPreviewTheme> {
  const htmlUri = vscode.Uri.joinPath(context.extensionUri, "out", "frontend", "index.html");
  const resourceRoot = vscode.Uri.joinPath(context.extensionUri, "out", "frontend");
  const html =
    builtinPreviewSourceMode === "tinymist"
      ? await tinymist.getResource("/preview/index.html")
      : await loadHTMLFile(context, "./out/frontend/index.html");

  if (typeof html !== "string") {
    throw new Error("Failed to load built-in preview HTML");
  }

  return {
    html,
    htmlUri,
    localResourceRoots: [resourceRoot],
    source: {
      kind: "builtin",
      trusted: vscode.workspace.isTrusted,
      htmlPath: htmlUri.fsPath,
    },
  };
}

async function resolveHtmlPreviewTheme(
  environment: PreviewThemeResolverEnvironment,
  provider: string | undefined,
  htmlPath: string,
): Promise<ResolvedPreviewTheme> {
  if (!htmlPath) {
    return fallbackToBuiltin(
      environment,
      provider,
      "did not include an HTML path after the `html:` prefix",
    );
  }

  if (!path.isAbsolute(htmlPath)) {
    return fallbackToBuiltin(
      environment,
      provider,
      "must resolve to an absolute HTML path after variable substitution",
    );
  }

  const htmlUri = vscode.Uri.file(htmlPath);
  return resolveHtmlFileTheme(environment, {
    provider,
    kind: "html",
    htmlUri,
    source: {
      kind: "html",
      trusted: environment.workspaceTrusted,
      configuredProvider: provider,
      htmlPath,
    },
  });
}

async function resolveExtensionPreviewTheme(
  environment: PreviewThemeResolverEnvironment,
  provider: string | undefined,
  extensionId: string,
): Promise<ResolvedPreviewTheme> {
  const extension = environment.getExtension?.(extensionId);
  if (!extension) {
    return fallbackToBuiltin(
      environment,
      provider,
      `could not find preview provider extension \`${extensionId}\``,
    );
  }

  let providerExports: unknown;
  try {
    providerExports = await extension.activate();
  } catch (error) {
    return fallbackToBuiltin(
      environment,
      provider,
      `failed to activate preview provider extension \`${extensionId}\`: ${errorMessage(error)}`,
    );
  }

  if (!isPreviewThemeProvider(providerExports)) {
    return fallbackToBuiltin(
      environment,
      provider,
      `extension \`${extensionId}\` does not export a \`provideTheme()\` preview provider`,
    );
  }

  let theme: TinymistPreviewTheme;
  try {
    theme = await providerExports.provideTheme();
  } catch (error) {
    return fallbackToBuiltin(
      environment,
      provider,
      `extension \`${extensionId}\` failed while providing a preview theme: ${errorMessage(error)}`,
    );
  }

  if (!theme || typeof theme.htmlPath !== "string" || theme.htmlPath.trim() === "") {
    return fallbackToBuiltin(
      environment,
      provider,
      `extension \`${extensionId}\` returned an empty preview HTML path`,
    );
  }

  if (
    typeof theme.compatibleTinymistVersion !== "string" ||
    theme.compatibleTinymistVersion.trim() === ""
  ) {
    return fallbackToBuiltin(
      environment,
      provider,
      `extension \`${extensionId}\` did not declare \`compatibleTinymistVersion\``,
    );
  }

  let isCompatible = false;
  try {
    isCompatible = theme.isCompatible
      ? await theme.isCompatible(environment.tinymistVersion)
      : theme.compatibleTinymistVersion === environment.tinymistVersion;
  } catch (error) {
    return fallbackToBuiltin(
      environment,
      provider,
      `extension \`${extensionId}\` failed while checking compatibility: ${errorMessage(error)}`,
    );
  }

  if (!isCompatible) {
    return fallbackToBuiltin(
      environment,
      provider,
      `extension \`${extensionId}\` is not compatible with Tinymist ${environment.tinymistVersion}`,
    );
  }

  const resolvedHtmlPath = path.isAbsolute(theme.htmlPath)
    ? theme.htmlPath
    : path.resolve(extension.extensionUri.fsPath, theme.htmlPath);
  const htmlUri = vscode.Uri.file(resolvedHtmlPath);

  return resolveHtmlFileTheme(environment, {
    provider,
    kind: "extension",
    htmlUri,
    source: {
      kind: "extension",
      trusted: environment.workspaceTrusted,
      configuredProvider: provider,
      extensionId,
      htmlPath: resolvedHtmlPath,
      compatibleTinymistVersion: theme.compatibleTinymistVersion,
    },
  });
}

async function resolveHtmlFileTheme(
  environment: PreviewThemeResolverEnvironment,
  options: {
    provider: string | undefined;
    kind: "html" | "extension";
    htmlUri: vscode.Uri;
    source: PreviewThemeSourceMetadata;
  },
): Promise<ResolvedPreviewTheme> {
  let html: string;
  try {
    html = await (environment.readHtmlFile ?? readPreviewThemeHtml)(options.htmlUri);
  } catch (error) {
    return fallbackToBuiltin(
      environment,
      options.provider,
      `could not read preview HTML from \`${options.htmlUri.fsPath}\`: ${errorMessage(error)}`,
    );
  }

  const contractIssue = validatePreviewThemeHtml(html);
  if (contractIssue) {
    return fallbackToBuiltin(
      environment,
      options.provider,
      `${options.kind === "extension" ? "extension preview theme" : "preview HTML"} ${contractIssue}`,
    );
  }

  return {
    html,
    htmlUri: options.htmlUri,
    localResourceRoots: [vscode.Uri.file(path.dirname(options.htmlUri.fsPath))],
    source: options.source,
  };
}

async function fallbackToBuiltin(
  environment: PreviewThemeResolverEnvironment,
  provider: string | undefined,
  fallbackReason: string,
  showWarning = true,
): Promise<ResolvedPreviewTheme> {
  if (provider && showWarning) {
    environment.showWarning?.(
      `Tinymist preview provider \`${provider}\` ${fallbackReason}. Falling back to the built-in preview.`,
    );
  }

  const builtinTheme = await environment.builtinTheme();
  return {
    ...builtinTheme,
    source: {
      ...builtinTheme.source,
      trusted: environment.workspaceTrusted,
      configuredProvider: provider,
      fallbackReason,
    },
  };
}

async function readPreviewThemeHtml(uri: vscode.Uri): Promise<string> {
  return readFile(uri.fsPath, "utf8");
}

function validatePreviewThemeHtml(html: string): string | undefined {
  const missingMarkers = PREVIEW_THEME_CONTRACT_MARKERS.filter((marker) => !html.includes(marker));
  if (missingMarkers.length === 0) {
    return undefined;
  }

  return `is missing required Tinymist preview markers: ${missingMarkers.join(", ")}`;
}

function isPreviewThemeProvider(value: unknown): value is TinymistPreviewThemeProvider {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as TinymistPreviewThemeProvider).provideTheme === "function"
  );
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
