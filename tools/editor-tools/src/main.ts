// tinymist-app
import "./style.css";
import van from "vanjs-core";
import { setupVscodeChannel } from "./vscode";
import { TemplateGallery } from "./features/template-gallery";
import { Tracing } from "./features/tracing";
import { Summary } from "./features/summary";
import { Diagnostics } from "./features/diagnostics";

// const isDarkMode = () =>
//   window.matchMedia?.("(prefers-color-scheme: dark)").matches;

setupVscodeChannel();

type PageComponent = "template-gallery" | "tracing" | "summary" | "diagnostics";

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
  let mode = `editor-tools-args:{"page": "summary"}`;
  /// Remove the placeholder prefix.
  mode = mode.replace("editor-tools-args:", "");

  /// Return a `WsArgs` object.
  return JSON.parse(mode);
}

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
  default:
    throw new Error(`Unknown page: ${args.page}`);
}
