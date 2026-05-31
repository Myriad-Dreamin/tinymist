import { readFile } from "fs/promises";
import * as path from "path";
import * as vscode from "vscode";

import { resolvePreviewerValue } from "../config";
import { tinymist } from "../lsp";
import { loadHTMLFile } from "../util";

export type PreviewerSourceKind = "builtin" | "html" | "extension";

export interface PreviewerSourceMetadata {
  kind: PreviewerSourceKind;
  trusted: boolean;
  configuredProvider?: string;
  extensionId?: string;
  htmlPath?: string;
  compatibleTinymistVersion?: string;
  fallbackReason?: string;
}

export interface ResolvedPreviewer {
  html: string;
  htmlUri: vscode.Uri;
  localResourceRoots: vscode.Uri[];
  source: PreviewerSourceMetadata;
}

export interface TinymistPreviewer {
  htmlPath: string;
  compatibleTinymistVersion: string;
  isCompatible?(tinymistVersion: string): Promise<boolean> | boolean;
}

export interface TinymistPreviewerProvider {
  providePreviewer(): Promise<TinymistPreviewer> | TinymistPreviewer;
}

interface PreviewerExtension {
  extensionUri: vscode.Uri;
  activate(): Thenable<unknown>;
}

export interface PreviewerResolverEnvironment {
  provider?: string;
  builtinPreviewerId?: string;
  workspaceTrusted: boolean;
  tinymistVersion: string;
  builtinPreviewer: () => Promise<ResolvedPreviewer>;
  readHtmlFile?: (uri: vscode.Uri) => Promise<string>;
  getExtension?: (id: string) => PreviewerExtension | undefined;
  showWarning?: (message: string) => void;
}

type ResolutionFailureMode = "fallback" | "error";

const DEFAULT_PREVIEWER_EXTENSION_ID = "myriad-dreamin.tinymist";

const PREVIEWER_CONTRACT_MARKERS = [
  "ws://127.0.0.1:23625",
  "preview-arg:previewMode:Doc",
  "preview-arg:state:",
] as const;

let builtinPreviewSourceMode: "compat" | "tinymist" = "compat";
let cachedPreviewer: { key: string; previewer: ResolvedPreviewer } | undefined;
let lastWarningKey: string | undefined;

export function setPreviewBuiltinSourceMode(mode: "compat" | "tinymist") {
  if (builtinPreviewSourceMode === mode) {
    return;
  }
  builtinPreviewSourceMode = mode;
  invalidatePreviewerCache();
}

export function invalidatePreviewerCache() {
  cachedPreviewer = undefined;
  lastWarningKey = undefined;
}

export async function preloadPreviewer(context: vscode.ExtensionContext) {
  await resolvePreviewer(context);
}

export function getConfiguredPreviewer(): string | undefined {
  return resolvePreviewerValue(
    vscode.workspace.getConfiguration("tinymist").get<string>("previewer"),
  );
}

export function parsePreviewerProvider(
  value: string | undefined,
  builtinPreviewerId = DEFAULT_PREVIEWER_EXTENSION_ID,
):
  | { kind: "builtin" }
  | { kind: "html"; htmlPath: string }
  | { kind: "extension"; extensionId: string } {
  const provider = value?.trim();
  if (!provider || provider === builtinPreviewerId || provider === DEFAULT_PREVIEWER_EXTENSION_ID) {
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

export async function resolveConfiguredPreviewer(
  environment: PreviewerResolverEnvironment,
): Promise<ResolvedPreviewer> {
  const provider = environment.provider?.trim();
  const parsed = parsePreviewerProvider(provider, environment.builtinPreviewerId);

  if (!environment.workspaceTrusted && parsed.kind !== "builtin") {
    return fallbackToBuiltin(environment, provider, "workspace is not trusted", false);
  }

  switch (parsed.kind) {
    case "builtin":
      return resolveBuiltinPreviewerSelection(environment, provider);
    case "html":
      return resolveHtmlPreviewer(environment, provider, parsed.htmlPath);
    case "extension":
      return resolveExtensionPreviewer(environment, provider, parsed.extensionId);
  }
}

export async function resolvePreviewer(
  context: vscode.ExtensionContext,
): Promise<ResolvedPreviewer> {
  const provider = getConfiguredPreviewer();
  const builtinPreviewerId = String(context.extension?.id ?? DEFAULT_PREVIEWER_EXTENSION_ID);
  const cacheKey = JSON.stringify({
    builtinPreviewSourceMode,
    builtinPreviewerId,
    provider,
    trusted: vscode.workspace.isTrusted,
    version: context.extension.packageJSON.version,
  });
  if (cachedPreviewer?.key === cacheKey) {
    return cachedPreviewer.previewer;
  }

  const previewer = await resolveConfiguredPreviewer({
    provider,
    builtinPreviewerId,
    workspaceTrusted: vscode.workspace.isTrusted,
    tinymistVersion: String(context.extension.packageJSON.version),
    builtinPreviewer: () => resolveBuiltinPreviewer(context),
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

  cachedPreviewer = { key: cacheKey, previewer };
  return previewer;
}

async function resolveBuiltinPreviewer(
  context: vscode.ExtensionContext,
): Promise<ResolvedPreviewer> {
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

async function resolveBuiltinPreviewerSelection(
  environment: PreviewerResolverEnvironment,
  provider: string | undefined,
): Promise<ResolvedPreviewer> {
  const previewer = await environment.builtinPreviewer();
  if (!provider) {
    return previewer;
  }

  return {
    ...previewer,
    source: {
      ...previewer.source,
      trusted: environment.workspaceTrusted,
      configuredProvider: provider,
    },
  };
}

async function resolveHtmlPreviewer(
  environment: PreviewerResolverEnvironment,
  provider: string | undefined,
  htmlPath: string,
): Promise<ResolvedPreviewer> {
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
  return resolveHtmlFilePreviewer(environment, {
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

async function resolveExtensionPreviewer(
  environment: PreviewerResolverEnvironment,
  provider: string | undefined,
  extensionId: string,
): Promise<ResolvedPreviewer> {
  const extension = environment.getExtension?.(extensionId);
  if (!extension) {
    throw previewerResolutionError(
      provider,
      `could not find previewer provider extension \`${extensionId}\``,
    );
  }

  let providerExports: unknown;
  try {
    providerExports = await extension.activate();
  } catch (error) {
    throw previewerResolutionError(
      provider,
      `failed to activate previewer provider extension \`${extensionId}\`: ${errorMessage(error)}`,
    );
  }

  if (!isPreviewerProvider(providerExports)) {
    throw previewerResolutionError(
      provider,
      `extension \`${extensionId}\` does not export a \`providePreviewer()\` previewer provider`,
    );
  }

  let previewer: TinymistPreviewer;
  try {
    previewer = await providerExports.providePreviewer();
  } catch (error) {
    throw previewerResolutionError(
      provider,
      `extension \`${extensionId}\` failed while providing a previewer: ${errorMessage(error)}`,
    );
  }

  if (!previewer || typeof previewer.htmlPath !== "string" || previewer.htmlPath.trim() === "") {
    throw previewerResolutionError(
      provider,
      `extension \`${extensionId}\` returned an empty preview HTML path`,
    );
  }

  if (
    typeof previewer.compatibleTinymistVersion !== "string" ||
    previewer.compatibleTinymistVersion.trim() === ""
  ) {
    throw previewerResolutionError(
      provider,
      `extension \`${extensionId}\` did not declare \`compatibleTinymistVersion\``,
    );
  }

  let isCompatible = false;
  try {
    isCompatible = previewer.isCompatible
      ? await previewer.isCompatible(environment.tinymistVersion)
      : previewer.compatibleTinymistVersion === environment.tinymistVersion;
  } catch (error) {
    throw previewerResolutionError(
      provider,
      `extension \`${extensionId}\` failed while checking compatibility: ${errorMessage(error)}`,
    );
  }

  if (!isCompatible) {
    throw previewerResolutionError(
      provider,
      `extension \`${extensionId}\` is not compatible with Tinymist ${environment.tinymistVersion}`,
    );
  }

  const resolvedHtmlPath = path.isAbsolute(previewer.htmlPath)
    ? previewer.htmlPath
    : path.resolve(extension.extensionUri.fsPath, previewer.htmlPath);
  const htmlUri = vscode.Uri.file(resolvedHtmlPath);

  return resolveHtmlFilePreviewer(environment, {
    provider,
    kind: "extension",
    failureMode: "error",
    htmlUri,
    source: {
      kind: "extension",
      trusted: environment.workspaceTrusted,
      configuredProvider: provider,
      extensionId,
      htmlPath: resolvedHtmlPath,
      compatibleTinymistVersion: previewer.compatibleTinymistVersion,
    },
  });
}

async function resolveHtmlFilePreviewer(
  environment: PreviewerResolverEnvironment,
  options: {
    provider: string | undefined;
    kind: "html" | "extension";
    failureMode?: ResolutionFailureMode;
    htmlUri: vscode.Uri;
    source: PreviewerSourceMetadata;
  },
): Promise<ResolvedPreviewer> {
  const failureMode = options.failureMode ?? "fallback";
  let html: string;
  try {
    html = await (environment.readHtmlFile ?? readPreviewerHtml)(options.htmlUri);
  } catch (error) {
    return handlePreviewerResolutionFailure(
      environment,
      options.provider,
      `could not read preview HTML from \`${options.htmlUri.fsPath}\`: ${errorMessage(error)}`,
      failureMode,
    );
  }

  const contractIssue = validatePreviewerHtml(html);
  if (contractIssue) {
    return handlePreviewerResolutionFailure(
      environment,
      options.provider,
      `${options.kind === "extension" ? "extension previewer" : "preview HTML"} ${contractIssue}`,
      failureMode,
    );
  }

  return {
    html,
    htmlUri: options.htmlUri,
    localResourceRoots: [vscode.Uri.file(path.dirname(options.htmlUri.fsPath))],
    source: options.source,
  };
}

async function handlePreviewerResolutionFailure(
  environment: PreviewerResolverEnvironment,
  provider: string | undefined,
  reason: string,
  failureMode: ResolutionFailureMode,
): Promise<ResolvedPreviewer> {
  if (failureMode === "error") {
    throw previewerResolutionError(provider, reason);
  }

  return fallbackToBuiltin(environment, provider, reason);
}

async function fallbackToBuiltin(
  environment: PreviewerResolverEnvironment,
  provider: string | undefined,
  fallbackReason: string,
  showWarning = true,
): Promise<ResolvedPreviewer> {
  if (provider && showWarning) {
    environment.showWarning?.(
      `Tinymist previewer \`${provider}\` ${fallbackReason}. Falling back to the built-in preview.`,
    );
  }

  const builtinPreviewer = await environment.builtinPreviewer();
  return {
    ...builtinPreviewer,
    source: {
      ...builtinPreviewer.source,
      trusted: environment.workspaceTrusted,
      configuredProvider: provider,
      fallbackReason,
    },
  };
}

function previewerResolutionError(provider: string | undefined, reason: string): Error {
  if (provider) {
    return new Error(`Tinymist previewer \`${provider}\` ${reason}.`);
  }

  return new Error(`Tinymist previewer ${reason}.`);
}

async function readPreviewerHtml(uri: vscode.Uri): Promise<string> {
  return readFile(uri.fsPath, "utf8");
}

function validatePreviewerHtml(html: string): string | undefined {
  const missingMarkers = PREVIEWER_CONTRACT_MARKERS.filter((marker) => !html.includes(marker));
  if (missingMarkers.length === 0) {
    return undefined;
  }

  return `is missing required Tinymist preview markers: ${missingMarkers.join(", ")}`;
}

function isPreviewerProvider(value: unknown): value is TinymistPreviewerProvider {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as TinymistPreviewerProvider).providePreviewer === "function"
  );
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
