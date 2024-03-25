// tinymist-app
import "./style.css";
import van from "vanjs-core";
import { setupVscodeChannel } from "./vscode";
import { TemplateGallery } from "./features/template-gallery";
import { Tracing } from "./features/tracing";

// const isDarkMode = () =>
//   window.matchMedia?.("(prefers-color-scheme: dark)").matches;

setupVscodeChannel();

type PageComponent = "template-gallery" | "tracing";

interface Arguments {
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
  let mode = `editor-tools-args:{"page": "tracing"}`;
  /// Remove the placeholder prefix.
  mode = mode.replace("editor-tools-args:", "");

  /// Return a `WsArgs` object.
  return JSON.parse(mode);
}

const args = retrieveArgs();

switch (args.page) {
  case "template-gallery":
    van.add(document.querySelector("#tinymist-app")!, TemplateGallery());
    break;
  case "tracing":
    van.add(document.querySelector("#tinymist-app")!, Tracing());
    break;
  default:
    throw new Error(`Unknown page: ${args.page}`);
}
