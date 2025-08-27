import type { PackageInfo } from "../../../lsp";
import { substituteTemplateString } from "../../../util";
import { defineEditorTool } from "..";

interface DocsViewOptions {
  pkg: PackageInfo;
  content: string;
}

export default defineEditorTool<DocsViewOptions>({
  id: "docs",
  title: (opts) => `@${opts.pkg.namespace}/${opts.pkg.name}:${opts.pkg.version} (Docs)`,
  webviewPanelOptions: {
    enableFindWidget: true,
  },

  transformHtml: (html, { opts }) => {
    return substituteTemplateString(html, {
      ":[[preview:DocContent]]:": opts.content as string,
    });
  },
});
