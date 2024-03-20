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
const BRACE_FREE_EXPR = /[^\s\}\{][^\}\{]*/.source;
const BRACE_AWARE_EXPR = BRACE_FREE_EXPR + `|(?:${CODE_BLOCK})`;

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

/**
 * Matches a (strict grammar) if in markup context.
 */
const strictIf = (): textmate.Grammar => {
  const ifStatement: textmate.Pattern = {
    name: "meta.expr.if.typst",
    begin: lookAhead(
      new RegExp(
        /(else\b)?(if\b)\s+/.source + `(?:${BRACE_AWARE_EXPR})` + /\s*\{/.source
      )
    ),
    end: /(?<=\})(?!\s*else)/,
    patterns: [
      /// Matches any comments
      {
        include: "#comments",
      },
      // todo
      /// Matches if clause with a code block expression
      /// Matches if clause
      {
        include: "#ifClause",
      },
      /// Matches else clause
      {
        include: "#elseClause",
      },
      /// Matches a code block after the if clause
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

  return {
    repository: {
      ifStatement,
      ifClause,
      elseClause,
      continuousCodeBlock,
    },
  };
};

const strictFor = (): textmate.Grammar => {
  // for v in expr { ... }
  const forStatement: textmate.Pattern = {
    name: "meta.expr.for.typst",
    begin: lookAhead(
      new RegExp(
        /(for\b)\s*/.source +
          `(?:${BRACE_FREE_EXPR})\\s*(in)\\s*(?:${BRACE_AWARE_EXPR})` +
          /\{/.source
      )
    ),
    end: /(?<=\})/,
    patterns: [
      /// Matches any comments
      {
        include: "#comments",
      },
      /// Matches for clause
      {
        include: "#forClause",
      },
      /// Matches a code block after the for clause
      {
        include: "#continuousCodeBlock",
      },
    ],
  };

  const forClause: textmate.Pattern = {
    // name: "meta.for.clause.bind.typst",
    // todo: consider comment in for /* {} */ in .. {}
    begin: new RegExp(/(for\b)\s*/.source + `(${BRACE_FREE_EXPR})\\s*(in)\\s*`),
    end: /(?=;|$|]|\}|\{)/,
    beginCaptures: {
      "1": {
        name: "keyword.control.loop.typst",
      },
      "2": {
        patterns: [
          {
            include: "#comments",
          },
          {
            include: "#pattern-binding-items",
          },
          {
            include: "#identifier",
          },
        ],
      },
      "3": {
        name: "keyword.operator.range.typst",
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

  return {
    repository: {
      forStatement,
      forClause,
    },
  };
};

export const typst: textmate.Grammar = {
  repository: {
    ...strictIf().repository,
    ...strictFor().repository,
    continuousCodeBlock,
  },
};

function generate() {
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
}

// console.log(typst!.repository!.forStatement);
generate();
