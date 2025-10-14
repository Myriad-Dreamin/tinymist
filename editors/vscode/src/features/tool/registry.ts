import type { EditorTool } from "../../tools";
import docsView from "../../tools/docs-view";
import exporter from "../../tools/exporter";
import fontView from "../../tools/font-view";
import profileServer from "../../tools/profile-server";
import summary from "../../tools/summary";
import symbolView from "../../tools/symbol-view";
import templateGallery from "../../tools/template-gallery";
import tracing from "../../tools/tracing";

export const tools: EditorTool[] = [
  templateGallery,
  summary,
  tracing,
  profileServer,
  fontView,
  symbolView,
  docsView,
  exporter,
];
