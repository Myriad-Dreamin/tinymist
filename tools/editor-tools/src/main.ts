import { mainHarness } from "./main.base";
import { TemplateGallery } from "./features/template-gallery";
import { Tracing } from "./features/tracing";
import { Summary } from "./features/summary";
import { Diagnostics } from "./features/diagnostics";
import { Docs } from "./features/docs";
import Exporter from "./features/exporter";
import FontView from "./features/font-view";

mainHarness({
  "template-gallery": TemplateGallery,
  tracing: Tracing(false),
  "profile-server": Tracing(true),
  summary: Summary,
  diagnostics: Diagnostics,
  "font-view": FontView,
  docs: Docs,
  exporter: Exporter,
});
