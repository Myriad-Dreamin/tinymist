import * as os from "node:os";
import * as path from "node:path";
import * as vscode from "vscode";

import { resolvePreviewProviderValue } from "../../config";
import { resolveConfiguredPreviewTheme, type ResolvedPreviewTheme } from "../../features/previewer";
import type { Context } from ".";

const TINYMIST_EXTENSION_ID = "myriad-dreamin.tinymist";
const FIXTURE_EXTENSION_ID = "myriad-dreamin.tinymist-previewer-fixture";

function builtinTheme(): ResolvedPreviewTheme {
  const htmlUri = vscode.Uri.file(
    path.join(os.tmpdir(), "tinymist-preview-theme-builtin", "index.html"),
  );
  return {
    html: "<html><body>builtin</body></html>",
    htmlUri,
    localResourceRoots: [vscode.Uri.file(path.dirname(htmlUri.fsPath))],
    source: {
      kind: "builtin",
      trusted: true,
      htmlPath: htmlUri.fsPath,
    },
  };
}

export async function getTests(ctx: Context) {
  await ctx.suite("preview provider resolver", async (suite) => {
    suite.addTest("parses html providers with workspace substitution", async () => {
      const workspaceUri = ctx.workspaceUri();
      const resolvedProvider = resolvePreviewProviderValue(
        "html:${workspaceFolder}/preview-provider/local-theme.html",
      );
      ctx
        .expect(resolvedProvider)
        .to.be.equal(
          `html:${vscode.Uri.joinPath(workspaceUri, "preview-provider", "local-theme.html").fsPath}`,
        );

      const result = await resolveConfiguredPreviewTheme({
        provider: resolvedProvider,
        workspaceTrusted: true,
        tinymistVersion: String(
          vscode.extensions.getExtension(TINYMIST_EXTENSION_ID)?.packageJSON.version ?? "0.0.0",
        ),
        builtinTheme: async () => builtinTheme(),
      });

      ctx.expect(result.source.kind).to.be.equal("html");
      ctx
        .expect(result.source.htmlPath)
        .to.be.equal(
          vscode.Uri.joinPath(workspaceUri, "preview-provider", "local-theme.html").fsPath,
        );
      ctx.expect(result.html).to.include("Hello from the local html preview provider");
    });

    suite.addTest("selects extension id preview providers", async () => {
      const tinymistVersion = String(
        vscode.extensions.getExtension(TINYMIST_EXTENSION_ID)?.packageJSON.version ?? "0.0.0",
      );

      const result = await resolveConfiguredPreviewTheme({
        provider: FIXTURE_EXTENSION_ID,
        workspaceTrusted: true,
        tinymistVersion,
        builtinTheme: async () => builtinTheme(),
        getExtension: (id) => vscode.extensions.getExtension(id),
      });

      ctx.expect(result.source.kind).to.be.equal("extension");
      ctx.expect(result.source.extensionId).to.be.equal(FIXTURE_EXTENSION_ID);
      ctx.expect(result.source.compatibleTinymistVersion).to.be.equal(tinymistVersion);
      ctx.expect(result.html).to.include("Hello from the fixture preview provider");
    });

    suite.addTest("falls back when workspace is untrusted", async () => {
      const result = await resolveConfiguredPreviewTheme({
        provider: FIXTURE_EXTENSION_ID,
        workspaceTrusted: false,
        tinymistVersion: "0.0.0",
        builtinTheme: async () => builtinTheme(),
        getExtension: (id) => vscode.extensions.getExtension(id),
      });

      ctx.expect(result.source.kind).to.be.equal("builtin");
      ctx.expect(result.source.configuredProvider).to.be.equal(FIXTURE_EXTENSION_ID);
      ctx.expect(result.source.fallbackReason).to.be.equal("workspace is not trusted");
    });

    suite.addTest("falls back when extension compatibility does not match", async () => {
      const result = await resolveConfiguredPreviewTheme({
        provider: FIXTURE_EXTENSION_ID,
        workspaceTrusted: true,
        tinymistVersion: "0.0.0-mismatch",
        builtinTheme: async () => builtinTheme(),
        getExtension: (id) => vscode.extensions.getExtension(id),
      });

      ctx.expect(result.source.kind).to.be.equal("builtin");
      ctx.expect(result.source.configuredProvider).to.be.equal(FIXTURE_EXTENSION_ID);
      ctx.expect(result.source.fallbackReason).to.include("is not compatible with Tinymist");
    });
  });
}
