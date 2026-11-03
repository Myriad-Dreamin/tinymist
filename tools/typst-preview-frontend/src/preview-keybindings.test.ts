import { describe, expect, it } from "vitest";
import { PreviewMode } from "typst-dom/typst-doc.mjs";
import { resolvePreviewKey } from "./preview-keybindings";

const key = (
  k: string,
  mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean }> = {},
) => ({
  key: k,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  ...mods,
});

describe("resolvePreviewKey", () => {
  it("maps bare navigation keys to their actions in Doc mode", () => {
    expect(resolvePreviewKey(key("j"), PreviewMode.Doc)).toEqual({
      action: "scrollDown",
      preventDefault: true,
    });
    expect(resolvePreviewKey(key("l"), PreviewMode.Doc)).toEqual({
      action: "scrollPageDown",
      preventDefault: true,
    });
    expect(resolvePreviewKey(key("t"), PreviewMode.Doc)).toEqual({
      action: "toggleTheme",
      preventDefault: true,
    });
  });

  it("does not preventDefault or act on Escape", () => {
    expect(resolvePreviewKey(key("Escape"), PreviewMode.Doc)).toEqual({
      action: "escape",
      preventDefault: false,
    });
  });

  it("ignores keys held with a modifier so browser shortcuts work", () => {
    for (const mod of ["ctrlKey", "metaKey", "altKey"] as const) {
      const r = resolvePreviewKey(key("l", { [mod]: true }), PreviewMode.Doc);
      expect(r).toEqual({ action: null, preventDefault: false });
    }
  });

  it("lets Ctrl+L through to the browser", () => {
    const r = resolvePreviewKey(key("l", { ctrlKey: true }), PreviewMode.Doc);
    expect(r.preventDefault).toBe(false);
    expect(r.action).toBeNull();
  });

  it("only navigates slides from arrow keys in Slide mode", () => {
    expect(resolvePreviewKey(key("ArrowRight"), PreviewMode.Slide)).toEqual({
      action: "next",
      preventDefault: true,
    });
    expect(resolvePreviewKey(key("ArrowRight"), PreviewMode.Doc)).toEqual({
      action: null,
      preventDefault: true,
    });
  });
});
