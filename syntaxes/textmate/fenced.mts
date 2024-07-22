import * as textmate from "./textmate.mjs";

const IDENTIFIER_BARE = /[\p{XID_Start}_][\p{XID_Continue}_\-]*/u;


export const inlineRaw: textmate.Pattern = {
  name: "markup.raw.inline.typst",
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
