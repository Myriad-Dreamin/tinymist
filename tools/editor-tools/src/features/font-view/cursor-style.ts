import van from "vanjs-core";
import { base64Decode } from "@/utils";
import { type SelectionStyle, type StyleAtCursor, styleAtCursor } from "@/vscode";

export function useStyleAtCursor() {
  const stub = `:[[preview:StyleAtCursor]]:`;

  const lastStyleAtCursor = van.state<StyleAtCursor | undefined>(
    stub.startsWith(":") ? undefined : JSON.parse(base64Decode(stub)),
  );

  van.derive(() => {
    const version = styleAtCursor.val?.version;
    if (version) {
      console.log("styleAtCursor", styleAtCursor, lastStyleAtCursor);
      const lastVersion = lastStyleAtCursor.val?.version;
      if (version && (typeof lastVersion !== "number" || lastVersion < version)) {
        lastStyleAtCursor.val = styleAtCursor.val;
      }
    }
  });

  const style = van.derive<SelectionStyle | undefined>(() => {
    if (typeof lastStyleAtCursor.val?.version !== "number") {
      return;
    }
    console.log("lastStyleAtCursor", lastStyleAtCursor);

    return lastStyleAtCursor.val.selections[0];
  });

  const fontAtCursor = van.derive(() => {
    const resolved = style.val?.styleAt?.[0].toString() || "";
    console.log("style", style, resolved);
    return resolved;
  });

  const fontPosition = van.derive(() => {
    if (style.val?.textDocument?.uri && style.val?.position) {
      return `${style.val.textDocument.uri}:${style.val.position.line}:${style.val.position.character}`;
    }
    return "";
  });

  return { fontAtCursor, fontPosition };
}
