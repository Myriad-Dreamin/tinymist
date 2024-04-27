import "./style.css";
import van from "vanjs-core";
import { setupVscodeChannel } from "./vscode";
import { TemplateGallery } from "./features/template-gallery";
import { Tracing } from "./features/tracing";
import { Summary } from "./features/summary";
import { Diagnostics } from "./features/diagnostics";
import { SymbolPicker } from "./features/symbol-view";

/// The components that can be rendered by the frontend.
/// Typically, each component corresponds to a single tool (Application).
type PageComponent =
  | "template-gallery"
  | "tracing"
  | "summary"
  | "diagnostics"
  | "symbol-view";

/// The frontend arguments that are passed from the backend.
interface Arguments {
  /// The page to render.
  page: PageComponent;
}

/// Placeholders for editor-tools program initializing frontend
/// arguments.
function retrieveArgs(): Arguments {
  /// The string `editor-tools-args:{}` is a placeholder
  /// It will be replaced by the actual arguments.
  /// ```rs
  ///   let frontend_html = frontend_html.replace(
  ///     "editor-tools-args:{}", ...);
  /// ```
  let mode = `editor-tools-args:{"page": "symbol-view"}`;
  /// Remove the placeholder prefix.
  mode = mode.replace("editor-tools-args:", "");

  /// Return a `WsArgs` object.
  return JSON.parse(mode);
}

function main() {
  setupVscodeChannel();

  const args = retrieveArgs();
  const appHook = document.querySelector("#tinymist-app")!;

  switch (args.page) {
    case "template-gallery":
      van.add(appHook, TemplateGallery());
      break;
    case "tracing":
      van.add(appHook, Tracing());
      break;
    case "summary":
      van.add(appHook, Summary());
      break;
    case "diagnostics":
      van.add(appHook, Diagnostics());
      break;
    case "symbol-view":
      van.add(appHook, SymbolPicker());
      break;
    default:
      throw new Error(`Unknown page: ${args.page}`);
  }
}

main();
