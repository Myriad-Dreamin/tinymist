import "./style.css";
import van, { ChildDom } from "vanjs-core";
import { setupVscodeChannel } from "./vscode";

/// Theme configuration for testing purposes
/// Available themes: "classic", "material-ui"
/// Available variants: "light", "dark"
const THEME_CONFIG = {
  theme: "classic" as "classic" | "material-ui",
  variant: "dark" as "light" | "dark",
};

/// Apply theme to document root
function applyTheme() {
  const root = document.documentElement;

  // Remove existing theme classes
  root.classList.remove("light", "dark", "classic", "material-ui");

  // Apply theme and variant classes
  root.classList.add(THEME_CONFIG.variant);
  if (THEME_CONFIG.theme !== "classic") {
    root.classList.add(THEME_CONFIG.theme);
  }
}

/// The components that can be rendered by the frontend.
/// Typically, each component corresponds to a single tool (Application).
type PageComponent =
  | "template-gallery"
  | "tracing"
  | "profile-server"
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

  // Apply theme configuration
  applyTheme();

  const args = retrieveArgs();
  const appHook = document.querySelector("#tinymist-app")!;

  const Component = components[args.page];
  if (!Component) {
    throw new Error(`Unknown page: ${args.page}`);
  }
  van.add(appHook, Component());
}
