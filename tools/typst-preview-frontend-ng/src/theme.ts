import type { InvertColorStrategy, InvertColorStrategyMap } from "./types";

export class InvertColorController {
  private autoDecision: boolean | undefined;

  apply(root: HTMLElement, strategy: InvertColorStrategy | InvertColorStrategyMap) {
    const target = typeof strategy === "string" ? { rest: strategy } : strategy;
    const decide = (value: InvertColorStrategy | undefined) => {
      switch (value || "never") {
        case "always":
          return true;
        case "auto":
          return (this.autoDecision ??= determineInvertColor());
        case "never":
        default:
          return false;
      }
    };
    root.classList.toggle("invert-colors", decide(target.rest));
    root.classList.toggle("normal-image", !decide(target.image || target.rest));
  }
}

function determineInvertColor() {
  const cls = document.body.classList;
  return (
    (cls.contains("vscode-dark") || cls.contains("vscode-high-contrast")) &&
    !cls.contains("vscode-light")
  );
}
