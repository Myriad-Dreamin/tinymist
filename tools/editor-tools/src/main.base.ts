import "./style.css";
import van, { ChildDom } from "vanjs-core";
import { setupVscodeChannel } from "./vscode";

/// The components that can be rendered by the frontend.
/// Typically, each component corresponds to a single tool (Application).
type PageComponent =
  | "template-gallery"
  | "tracing"
  | "summary"
  | "diagnostics"
  | "symbol-view"
  | "font-view"
  | "docs";

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
  let mode = `editor-tools-args:{"page": "font-view"}`;
  /// Remove the placeholder prefix.
  mode = mode.replace("editor-tools-args:", "");

  /// Return a `WsArgs` object.
  return JSON.parse(mode);
}

type Registry = Partial<Record<PageComponent, () => ChildDom>>;
export function mainHarness(components: Registry) {
  setupVscodeChannel();

  const args = retrieveArgs();
  const appHook = document.querySelector("#tinymist-app")!;

  const Component = components[args.page];
  if (!Component) {
    throw new Error(`Unknown page: ${args.page}`);
  }
  van.add(appHook, Component());
}
