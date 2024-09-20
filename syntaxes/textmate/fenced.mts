import * as textmate from "./textmate.mjs";
import { languages as rawLanguages } from "./fenced.meta.mjs";

const IDENTIFIER_BARE = /[\p{XID_Start}_][\p{XID_Continue}_\-]*/u;

export interface Lang {
  as?: string | string[];
  candidates: string[];
}

const genLang = (
  langMeta: Lang
): textmate.PatternInclude & { lang: string } => {
  const lang = langMeta.candidates[0];
  let includes = langMeta.as;
  if (!includes) {
    includes = [`source.${lang}`];
  } else if (typeof includes === "string") {
    includes = [includes];
  }

  const sourcePatterns = includes.map((include) => ({ include }));
  const candidates = langMeta.candidates.map((s) =>
    s.replace(/[.+]/g, (e) => `\\${e}`)
  );

  const enter = (n: number): textmate.Pattern => ({
    name: `markup.raw.block.typst`,
    begin: new RegExp(
      "(`{" + n.toString() + "})" + `(${candidates.join("|")})\\b`
    ),
    beginCaptures: {
      "1": {
        name: "punctuation.definition.raw.begin.typst",
      },
      "2": {
        name: "fenced_code.block.language.typst",
      },
    },
    end: /\s*(\1)/,
    endCaptures: {
      "1": {
        name: "punctuation.definition.raw.end.typst",
      },
    },
    patterns: [
      {
        begin: /(^|\G)(\s*)/,
        // end: "(?=`{" + n.toString() + ",})",
        while: "(^|\\G)(?!\\s*`{" + n.toString() + ",}\\s*)",
        contentName: `meta.embedded.block.${lang}`,
        patterns: sourcePatterns,
      },
    ],
  });

  return {
    lang,
    patterns: [
      // one line case
      {
        name: `markup.raw.block.typst`,
        match: new RegExp(
          /(`{3,})/.source + `(${candidates.join("|")})` + /\b(.*?)(\1)/.source
        ),
        captures: {
          "1": {
            name: "punctuation.definition.raw.begin.typst",
          },
          "2": {
            name: "fenced_code.block.language.typst",
          },
          "3": {
            name: `meta.embedded.block.${lang}`,
            patterns: sourcePatterns,
          },
          "4": {
            name: "punctuation.definition.raw.end.typst",
          },
        },
      },
      ...[6, 5, 4, 3].map(enter),
    ],
  };
};

const RENDER_LANGS = true;
export const blockRawLangs = RENDER_LANGS ? rawLanguages.map(genLang) : [];

export const inlineRaw: textmate.Pattern = {
  name: "markup.raw.inline.typst string.other.raw.typst",
  begin: /`/,
  end: /`/,
  beginCaptures: {
    "0": {
      name: "punctuation.definition.raw.inline.typst",
    },
  },
  endCaptures: {
    "0": {
      name: "punctuation.definition.raw.inline.typst",
    },
  },
};

export const blockRaw: textmate.Pattern = {
  patterns: [
    ...blockRawLangs.map((blockRawLang) => ({
      include: "#" + blockRawLang.lang.replace(/\./g, "_"),
    })),
    {
      include: "#blockRawGeneral",
    },
  ],
};

export const blockRawGeneral: textmate.Pattern = {
  name: "markup.raw.block.typst",
  begin: new RegExp(/(`{3,})/.source + `(${IDENTIFIER_BARE.source}\\b)?`),
  beginCaptures: {
    "1": {
      name: "punctuation.definition.raw.begin.typst",
    },
    "2": {
      name: "fenced_code.block.language.typst",
    },
  },
  end: /(\1)/,
  endCaptures: {
    "1": {
      name: "punctuation.definition.raw.end.typst",
    },
  },
};
