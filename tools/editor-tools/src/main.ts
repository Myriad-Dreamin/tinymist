import { mainHarness } from "./main.base";
import { TemplateGallery } from "./features/template-gallery";
import { Tracing } from "./features/tracing";
import { Summary } from "./features/summary";
import { Diagnostics } from "./features/diagnostics";
import { Docs } from "./features/docs";

mainHarness({
  "template-gallery": TemplateGallery,
  tracing: Tracing,
  summary: Summary,
  diagnostics: Diagnostics,
  docs: Docs,
});
