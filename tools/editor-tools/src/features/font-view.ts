import van, { ChildDom, PropsWithKnownKeys, State } from "vanjs-core";
import {
  copyToClipboard,
  requestRevealPath,
  requestTextEdit,
  styleAtCursor as stylesAtCursor,
} from "../vscode";
import { base64Decode } from "../utils";
import { FontSource, humanStretch, humanStyle, humanWeight } from "../types";
const { div, a, span, code, br, button } = van.tags;

export const FontView = () => {
  const showNumber = van.state(false);
  const showNumberOpt = van.derive(() => ({ showNumber: showNumber.val }));

  const FontResourcesData = `:[[preview:FontInformation]]:`;
  const fontResources = van.state<FontResources>(
    FontResourcesData.startsWith(":")
      ? DOC_MOCK
      : JSON.parse(base64Decode(FontResourcesData))
  );
  console.log("fontResources", fontResources);

  const StyleAtCursorData = `:[[preview:StyleAtCursor]]:`;
  const lastStylesAtCursor = van.state<any>(
    StyleAtCursorData.startsWith(":")
      ? undefined
      : JSON.parse(base64Decode(StyleAtCursorData))
  );
  console.log("styleAtCursorBase", lastStylesAtCursor);
  van.derive(() => {
    if (stylesAtCursor.val && stylesAtCursor.val.version) {
      console.log("styleAtCursor", stylesAtCursor, lastStylesAtCursor);
      if (
        !(typeof lastStylesAtCursor.val?.version == "number") ||
        lastStylesAtCursor.val.version < stylesAtCursor.val?.version
      ) {
        lastStylesAtCursor.val = stylesAtCursor.val;
      }

      return;
    }
  });

  const style = van.derive<{
    textDocument: any;
    position: any;
    styleAt: any[];
  }>(() => {
    if (!(typeof lastStylesAtCursor.val?.version == "number")) {
      return {};
    }
    console.log("lastStylesAtCursor", lastStylesAtCursor);

    return lastStylesAtCursor.val.selections[0] || {};
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

  const FontAction = (
    icon: ChildDom,
    title: string,
    onclick: (this: HTMLDivElement) => void,
    opts?: PropsWithKnownKeys<HTMLButtonElement> & { active?: State<boolean> }
  ) => {
    const classProp = opts?.active
      ? van.derive(
          () =>
            `tinymist-button tinymist-font-action${opts?.active?.val ? " activated" : ""}`
        )
      : "tinymist-button tinymist-font-action";

    return button(
      {
        ...opts,
        class: classProp,
        style: "height: 1.2rem",
        title,
        onclick,
      },
      icon
    );
  };

  const FontSlot = (font: FontInfo) => {
    let fileName;
    if (typeof font.source === "number") {
      let w = fontResources.val.sources[font.source];
      if (w.kind === "fs") {
        fileName = w.path.split(/[\\\/]/g).pop();
      } else {
        fileName = `Embedded: ${w.name}`;
      }
    }

    const machineTitle = `Weight ${font.weight || 400}, Stretch ${font.stretch || 1000}, at `;
    const baseName = code(
      font.style === "normal" || !font.style
        ? ""
        : `${humanStyle(font.style)}, `,
      (_dom?: Element) => {
        return span(
          humanWeight(font.weight, showNumberOpt.val),
          showNumber.val ? ", " : " ",
          humanStretch(font.stretch, showNumberOpt.val)
        );
      },
      ` (${fileName})`
    );

    let variantName;
    if (typeof font.source === "number") {
      let w = fontResources.val.sources[font.source];
      let title;
      if (w.kind === "fs") {
        title = machineTitle + w.path;
        variantName = a(
          {
            style:
              "font-size: 1.2em; text-decoration: underline; cursor: pointer;",
            title,
            onclick() {
              if (w.kind === "fs") {
                requestRevealPath(w.path);
              }
            },
          },
          baseName
        );
      } else {
        title = machineTitle + `Embedded: ${w.name}`;
        variantName = span(
          {
            style: "font-size: 1.2em",
            title,
          },
          baseName
        );
      }
    } else {
      variantName = a({ style: "font-size: 1.2em" }, baseName);
    }

    // br(),
    // code("PostScriptName"),
    // ": ",
    // code(font.postscriptName),
    // br(),
    // code(
    //   font.fixedFamily === font.family
    //     ? "Family"
    //     : "Family (Identified by Typst)"
    // ),
    // ": ",
    // code(
    //   font.fixedFamily === font.family
    //     ? font.family
    //     : `${font.family} (${font.fixedFamily})`
    // )
    return div({ style: "margin-left: 0.5em" }, variantName);
  };
  function activeMe(d: HTMLDivElement) {
    d.classList.add("active");
    setTimeout(() => d.classList.remove("active"), 500);
  }

  const FontFamilySlot = (family: FontFamily) => {
    const name = `"${family.name || ""}"`;

    return div(
      { class: `tinymist-card`, style: "flex: 1; width: 100%; padding: 10px" },
      (_dom?: Element) =>
        div(
          { style: "margin: 1.2em; margin-left: 0.5em" },
          div(
            FontAction(
              "Copy",
              "Copy to clipboard",
              function (this: HTMLDivElement) {
                activeMe(this);
                copyToClipboard(`"${family.name || ""}"`);
              }
            ),
            " | ",
            FontAction(
              "Paste string",
              "Paste as String",
              function (this: HTMLDivElement) {
                activeMe(this);
                const rest = name;
                const markup = `#${rest}`;
                requestTextEdit({
                  newText: {
                    kind: "by-mode",
                    markup,
                    rest,
                  },
                });
              }
            ),
            " ",
            FontAction(
              "#set",
              "Paste as Set Font Rule",
              function (this: HTMLDivElement) {
                activeMe(this);
                const rest = name;
                const markup = `#set text(font: ${rest})`;
                requestTextEdit({
                  newText: {
                    kind: "by-mode",
                    markup,
                    rest,
                  },
                });
              }
            )
          ),
          span({ style: "font-size: 1.2em" }, family.name),
          ".",
          br(),
          code("Variant"),
          ": ",
          family.infos.map(FontSlot)
        )
    );
  };

  // todo: very buggy so we disabling it
  const SelectingSlot = () => {
    return div(
      { style: "margin: 1.2em; margin-left: 0.5em" },
      "The font of selecting content is ",
      code(fontAtCursor),
      br(),
      "Checked at ",
      code(fontPosition)
    );
  };

  return div(
    {
      style: "width: 100%;",
    },
    (_dom?: Element) =>
      div(
        {
          class: "flex-col",
          style:
            "justify-content: center; align-items: center; gap: 10px; width: 100%;",
        },
        div(
          {
            style: "flex: 1; width: 100%; padding: 10px",
          },
          FontAction(
            "Show Number",
            "Toggle to show weight or stretch number",
            () => {
              showNumber.val = !showNumber.val;
            },
            { active: showNumber }
          )
        ),
        div(
          {
            class: `tinymist-card`,
            style: "flex: 1; width: 100%; padding: 10px; display: none",
          },
          (_dom?: Element) => SelectingSlot()
        ),
        ...fontResources.val.families.map(FontFamilySlot)
      )
  );
};

export type fontLocation = FontSource extends { kind: infer Kind }
  ? Kind
  : never;

interface FontInfo {
  name: string;
  style?: string;
  weight?: number;
  stretch?: number;
  source?: number;
  index?: number;
}

interface FontFamily {
  name: string;
  infos: FontInfo[];
}

interface FontResources {
  sources: FontSource[];
  families: FontFamily[];
}

const DOC_MOCK: FontResources = {
  sources: [
    {
      kind: "fs",
      path: "C:\\Users\\OvO\\work\\assets\\fonts\\SongTi-Regular.ttf",
    },
    {
      kind: "fs",
      path: "C:\\Users\\OvO\\work\\assets\\fonts\\TimesNewRoman-Regular.ttf",
    },
    {
      kind: "fs",
      path: "C:\\Users\\OvO\\work\\assets\\fonts\\MicrosoftYaHei-Regular.ttf",
    },
  ],
  families: [
    {
      name: "Song Ti",
      infos: [
        {
          name: "Song Ti",
          source: 0,
        },
      ],
    },
    {
      name: "Times New Roman",
      infos: [
        {
          name: "Times New Roman",
          source: 1,
        },
      ],
    },
    {
      name: "Microsoft YaHei",
      infos: [
        {
          name: "Microsoft YaHei",
          source: 2,
        },
      ],
    },
  ],
};
