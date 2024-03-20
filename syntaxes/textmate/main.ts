import * as textmate from "./textmate";
import * as yaml from "js-yaml";
// JS-Snippet to generate pattern
function generatePattern(maxDepth: number) {
  const NOT_BRACE_PATTERN = "[^\\}\\{]";

  // Unrolled Pattern variants: 0=default, 1=unrolled (more efficient)
  let p = [`\\{${NOT_BRACE_PATTERN}*(?:`, `${NOT_BRACE_PATTERN}*)*\\}`];

  // Generate and display the pattern
  return (
    p[0].repeat(maxDepth) +
    `\\{${NOT_BRACE_PATTERN}*\\}` +
    p[1].repeat(maxDepth)
  );
}

function lookAhead(pattern: RegExp) {
  return new RegExp(`(?=(?:${pattern.source}))`);
}

const CODE_BLOCK = generatePattern(6);
const BRACE_AWARE_EXPR = /[^\s\}\{][^\}\{]*/.source + `|(?:${CODE_BLOCK})`;

const continuousCodeBlock: textmate.Pattern = {
  //   name: "meta.block.continuous.typst",
  begin: /\{/,
  end: /(\})/,
  beginCaptures: {
    "0": {
      name: "meta.brace.curly.typst",
    },
  },
  endCaptures: {
    "1": {
      name: "meta.brace.curly.typst",
    },
  },
  patterns: [
    {
      include: "#code",
    },
  ],
};

const ifStatement: textmate.Pattern = {
  name: "meta.expr.if.typst",
  begin: lookAhead(
    new RegExp(
      /(else\b)?(if)\s+/.source + `(?:${BRACE_AWARE_EXPR})` + /\s*\{/.source
    )
  ),
  end: /(?<=\})(?!\s*else)/,
  patterns: [
    /// Matches any comments
    {
      include: "#comments",
    },
    // todo
    /// Matches if statement with a code block expression
    /// Matches if statement
    {
      include: "#ifClause",
    },
    /// Matches else statement
    {
      include: "#elseClause",
    },
    /// Matches a code block after the if statement
    {
      include: "#continuousCodeBlock",
    },
  ],
};

const ifClause: textmate.Pattern = {
  //   name: "meta.if.clause.typst",
  begin: /(else\b)?(if)\s+/,
  end: /(?=;|$|]|\}|\{)/,
  beginCaptures: {
    "1": {
      name: "keyword.control.conditional.typst",
    },
    "2": {
      name: "keyword.control.conditional.typst",
    },
  },
  patterns: [
    {
      include: "#comments",
    },
    {
      include: "#code-expr",
    },
  ],
};

const elseClause: textmate.Pattern = {
  //   name: "meta.else.clause.typst",
  begin: /(\belse)\s*(\{)/,
  end: /\}/,
  beginCaptures: {
    "1": {
      name: "keyword.control.conditional.typst",
    },
    "2": {
      name: "meta.brace.curly.typst",
    },
  },
  endCaptures: {
    "0": {
      name: "meta.brace.curly.typst",
    },
  },
  patterns: [
    {
      include: "#code",
    },
  ],
};

export const typst: textmate.Grammar = {
  repository: {
    ifStatement,
    ifClause,
    elseClause,
    continuousCodeBlock,
  },
};

const fs = require("fs");
const path = require("path");

const typstPath = path.join(__dirname, "../typst.tmLanguage");

const base = fs.readFileSync(`${typstPath}.yaml`, "utf8");
const baseObj = yaml.load(base) as textmate.Grammar;

const compiled = textmate.compile(typst);
baseObj.repository = Object.assign(
  baseObj.repository || {},
  JSON.parse(compiled).repository
);

// dump to file
fs.writeFileSync(`${typstPath}.json`, JSON.stringify(baseObj));
