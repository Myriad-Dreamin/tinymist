import * as os from "node:os";
import * as path from "node:path";
import * as vscode from "vscode";

import { resolvePreviewerValue } from "../../config";
import { resolveConfiguredPreviewer, type ResolvedPreviewer } from "../../features/previewer";
import type { Context } from ".";

const TINYMIST_EXTENSION_ID = "myriad-dreamin.tinymist";
const FIXTURE_EXTENSION_ID = "myriad-dreamin.tinymist-previewer-fixture";

function builtinPreviewer(): ResolvedPreviewer {
  const htmlUri = vscode.Uri.file(
    path.join(os.tmpdir(), "tinymist-previewer-builtin", "index.html"),
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

async function expectPreviewerError(
  ctx: Context,
  action: Promise<ResolvedPreviewer>,
  expectedMessage: string,
) {
  let caught: unknown;
  try {
    await action;
  } catch (error) {
    caught = error;
  }

  ctx.expect(caught).to.be.instanceOf(Error);
  ctx.expect((caught as Error).message).to.include(expectedMessage);
}

export async function getTests(ctx: Context) {
  const workspaceCtx = ctx.workspaceCtx("book");

  await workspaceCtx.suite("previewer resolver", async (suite) => {
    suite.addTest("parses html previewers with workspace substitution", async () => {
      const workspaceUri = workspaceCtx.workspaceUri();
      const resolvedProvider = resolvePreviewerValue(
        "html:${workspaceFolder}/preview-provider/local-previewer.html",
      );
      workspaceCtx
        .expect(resolvedProvider)
        .to.be.equal(
          `html:${vscode.Uri.joinPath(workspaceUri, "preview-provider", "local-previewer.html").fsPath}`,
        );

      const result = await resolveConfiguredPreviewer({
        provider: resolvedProvider,
        workspaceTrusted: true,
        tinymistVersion: String(
          vscode.extensions.getExtension(TINYMIST_EXTENSION_ID)?.packageJSON.version ?? "0.0.0",
        ),
        builtinPreviewer: async () => builtinPreviewer(),
      });

      workspaceCtx.expect(result.source.kind).to.be.equal("html");
      workspaceCtx
        .expect(result.source.htmlPath)
        .to.be.equal(
          vscode.Uri.joinPath(workspaceUri, "preview-provider", "local-previewer.html").fsPath,
        );
      workspaceCtx.expect(result.html).to.include("Hello from the local html previewer");
    });

    suite.addTest("selects extension id previewers", async () => {
      const tinymistVersion = String(
        vscode.extensions.getExtension(TINYMIST_EXTENSION_ID)?.packageJSON.version ?? "0.0.0",
      );

      const result = await resolveConfiguredPreviewer({
        provider: FIXTURE_EXTENSION_ID,
        workspaceTrusted: true,
        tinymistVersion,
        builtinPreviewer: async () => builtinPreviewer(),
        getExtension: (id) => vscode.extensions.getExtension(id),
      });

      workspaceCtx.expect(result.source.kind).to.be.equal("extension");
      workspaceCtx.expect(result.source.extensionId).to.be.equal(FIXTURE_EXTENSION_ID);
      workspaceCtx.expect(result.source.compatibleTinymistVersion).to.be.equal(tinymistVersion);
      workspaceCtx.expect(result.html).to.include("Hello from the fixture previewer");
    });

    suite.addTest("uses the built-in previewer for Tinymist's own extension id", async () => {
      const result = await resolveConfiguredPreviewer({
        provider: TINYMIST_EXTENSION_ID,
        workspaceTrusted: true,
        tinymistVersion: "0.0.0",
        builtinPreviewer: async () => builtinPreviewer(),
        getExtension: (id) => vscode.extensions.getExtension(id),
      });

      workspaceCtx.expect(result.source.kind).to.be.equal("builtin");
      workspaceCtx.expect(result.source.configuredProvider).to.be.equal(TINYMIST_EXTENSION_ID);
      workspaceCtx.expect(result.source.fallbackReason).to.be.undefined;
    });

    suite.addTest("throws when extension id previewer cannot be found", async () => {
      const missingProvider = "myriad-dreamin.tinymist-missing-previewer";

      await expectPreviewerError(
        workspaceCtx,
        resolveConfiguredPreviewer({
          provider: missingProvider,
          workspaceTrusted: true,
          tinymistVersion: "0.0.0",
          builtinPreviewer: async () => {
            throw new Error("unexpected built-in previewer fallback");
          },
          getExtension: (id) => vscode.extensions.getExtension(id),
        }),
        `could not find previewer provider extension \`${missingProvider}\``,
      );
    });

    suite.addTest("falls back when workspace is untrusted", async () => {
      const result = await resolveConfiguredPreviewer({
        provider: FIXTURE_EXTENSION_ID,
        workspaceTrusted: false,
        tinymistVersion: "0.0.0",
        builtinPreviewer: async () => builtinPreviewer(),
        getExtension: (id) => vscode.extensions.getExtension(id),
      });

      workspaceCtx.expect(result.source.kind).to.be.equal("builtin");
      workspaceCtx.expect(result.source.configuredProvider).to.be.equal(FIXTURE_EXTENSION_ID);
      workspaceCtx.expect(result.source.fallbackReason).to.be.equal("workspace is not trusted");
    });

    suite.addTest("throws when extension compatibility does not match", async () => {
      await expectPreviewerError(
        workspaceCtx,
        resolveConfiguredPreviewer({
          provider: FIXTURE_EXTENSION_ID,
          workspaceTrusted: true,
          tinymistVersion: "0.0.0-mismatch",
          builtinPreviewer: async () => {
            throw new Error("unexpected built-in previewer fallback");
          },
          getExtension: (id) => vscode.extensions.getExtension(id),
        }),
        "is not compatible with Tinymist 0.0.0-mismatch",
      );
    });
  });
}
