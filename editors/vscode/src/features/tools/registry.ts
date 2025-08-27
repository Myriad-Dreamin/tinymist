import type { EditorTool } from "./index";
import templateGallery from "./tools/template-gallery";
import summary from "./tools/summary";
import tracing from "./tools/tracing";
import profileServer from "./tools/profile-server";
import fontView from "./tools/font-view";
import symbolView from "./tools/symbol-view";
import docsView from "./tools/docs-view";

export const tools : EditorTool[] = [
  templateGallery,
  summary,
  tracing,
  profileServer,
  fontView,
  symbolView,
  docsView,
]
