import * as textmate from "./textmate";
import * as yaml from "js-yaml";

// JS-Snippet to generate pattern
function generatePattern(maxDepth: number, lb: string, rb: string) {
  const NOT_BRACE_PATTERN = `[^${rb}${lb}]`;

  // Unrolled Pattern variants: 0=default, 1=unrolled (more efficient)
  let p = [`${lb}${NOT_BRACE_PATTERN}*(?:`, `${NOT_BRACE_PATTERN}*)*${rb}`];

  // Generate and display the pattern
  return (
    p[0].repeat(maxDepth) +
    `${lb}${NOT_BRACE_PATTERN}*${rb}` +
    p[1].repeat(maxDepth)
  );
}

function lookAhead(pattern: RegExp) {
  return new RegExp(`(?=(?:${pattern.source}))`);
}

function braceMatch(pattern: RegExp) {
  return ("(?x)" + pattern.source) as unknown as RegExp;
}

const PAREN_BLOCK = generatePattern(6, "\\(", "\\)");
const CODE_BLOCK = generatePattern(6, "\\{", "\\}");
const CONTENT_BLOCK = generatePattern(6, "\\[", "\\]");
const BRACE_FREE_EXPR = /[^\s\}\{\[\]][^\}\{\[\]]*/.source;
const BRACE_AWARE_EXPR =
  BRACE_FREE_EXPR +
  `(?:(?:${CODE_BLOCK})|(?:${CONTENT_BLOCK})${BRACE_FREE_EXPR})?`;

const continuousCodeBlock: textmate.Pattern = {
  //   name: "meta.block.continuous.typst",
  begin: /\{/,
  end: /\}/,
  beginCaptures: {
    "0": {
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

const continuousContentBlock: textmate.Pattern = {
  //   name: "meta.block.continuous.typst",
  begin: /\[/,
  end: /\]/,
  beginCaptures: {
    "0": {
      name: "meta.brace.square.typst",
    },
  },
  endCaptures: {
    "0": {
      name: "meta.brace.square.typst",
    },
  },
  patterns: [
    {
      include: "#markup",
    },
  ],
};

const primitiveTypes: textmate.PatternMatch = {
  match: /\b(auto|any|none|false|true|str|int|float|bool|length|content)\b/,
  name: "entity.name.type.primitive.typst",
};

const IDENTIFIER = /\b[\p{XID_Start}_][\p{XID_Continue}_-]*\b/;

// todo: distinguish type and variable
const identifier: textmate.PatternMatch = {
  match: IDENTIFIER,
  captures: {
    "0": {
      name: "variable.other.readwrite.typst",
    },
  },
};

/**
 * Matches a (strict grammar) if in markup context.
 */
const strictIf = (): textmate.Grammar => {
  const ifStatement: textmate.Pattern = {
    name: "meta.expr.if.typst",
    begin: lookAhead(
      new RegExp(
        /(else\b)?(if\b)\s+/.source +
          `(?:${BRACE_AWARE_EXPR})` +
          /\s*[\{\[]/.source
      )
    ),
    end: /(?<=\}|\])(?!\s*else)/,
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
      /// Matches else content clause
      {
        include: "#elseContentClause",
      },
      /// Matches a code block after the if clause
      {
        include: "#continuousCodeBlock",
      },
      /// Matches a content block after the if clause
      {
        include: "#continuousContentBlock",
      },
    ],
  };

  const ifClause: textmate.Pattern = {
    //   name: "meta.if.clause.typst",
    begin: /(else\b)?(if)\s+/,
    end: /(?=[;\[\]{}]|$)/,
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

  const elseContentClause: textmate.Pattern = {
    //   name: "meta.else.clause.typst",
    begin: /(\belse)\s*(\[)/,
    end: /\]/,
    beginCaptures: {
      "1": {
        name: "keyword.control.conditional.typst",
      },
      "2": {
        name: "meta.brace.square.typst",
      },
    },
    endCaptures: {
      "0": {
        name: "meta.brace.square.typst",
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
      elseContentClause,
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
          /\s*[\{\[]/.source
      )
    ),
    end: /(?<=\}|\])/,
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
      /// Matches a content block after the for clause
      {
        include: "#continuousContentBlock",
      },
    ],
  };

  const forClause: textmate.Pattern = {
    // name: "meta.for.clause.bind.typst",
    // todo: consider comment in for /* {} */ in .. {}
    begin: new RegExp(/(for\b)\s*/.source + `(${BRACE_FREE_EXPR})\\s*(in)\\s*`),
    end: /(?=[;\[\]{}]|$)/,
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

const strictWhile = (): textmate.Grammar => {
  // for v in expr { ... }
  const whileStatement: textmate.Pattern = {
    name: "meta.expr.while.typst",
    begin: lookAhead(
      new RegExp(
        /(while\b)\s*/.source + `(?:${BRACE_AWARE_EXPR})` + /\s*[\{\[]/.source
      )
    ),
    end: /(?<=\}|\])/,
    patterns: [
      /// Matches any comments
      {
        include: "#comments",
      },
      /// Matches while clause
      {
        include: "#whileClause",
      },
      /// Matches a code block after the while clause
      {
        include: "#continuousCodeBlock",
      },
      /// Matches a content block after the while clause
      {
        include: "#continuousContentBlock",
      },
    ],
  };

  const whileClause: textmate.Pattern = {
    // name: "meta.while.clause.bind.typst",
    begin: /(while\b)\s*/,
    end: /(?=[;\[\]{}]|$)/,
    beginCaptures: {
      "1": {
        name: "keyword.control.loop.typst",
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
      whileStatement,
      whileClause,
    },
  };
};

const setStatement = (): textmate.Grammar => {
  const setStatement: textmate.Pattern = {
    name: "meta.expr.set.typst",
    begin: lookAhead(new RegExp(/(set\b)\s*/.source + IDENTIFIER.source)),
    end: /(?<=\))(?!if)|(?=[\s\{\}\[\];])/,
    patterns: [
      /// Matches any comments
      {
        include: "#comments",
      },
      /// Matches binding clause
      {
        include: "#setClause",
      },
      /// Matches condition after the set clause
      {
        include: "#setIfClause",
      },
    ],
  };

  const setClause: textmate.Pattern = {
    // name: "meta.set.clause.bind.typst",
    begin: /(set\b)\s*/,
    end: /(?=if)|(?=[;\]}])/,
    beginCaptures: {
      "1": {
        name: "keyword.control.other.typst",
      },
    },
    patterns: [
      {
        include: "#comments",
      },
      /// Matches a func call after the set clause
      {
        include: "#funcCall",
      },
      {
        include: "#identifier",
      },
    ],
  };

  const setIfClause: textmate.Pattern = {
    // name: "meta.set.if.clause.cond.typst",
    begin: /(if)\s*/,
    end: /(?<=\S)|(?=[;\]}])/,
    beginCaptures: {
      "1": {
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

  return {
    repository: {
      setStatement,
      setClause,
      setIfClause,
    },
  };
};

// todo: { f }(..args)
// todo: ( f )(..args)
const callArgs: textmate.Pattern = {
  begin: /\(/,
  end: /\)/,
  beginCaptures: {
    "0": {
      name: "meta.brace.round.typst",
    },
  },
  endCaptures: {
    "0": {
      name: "meta.brace.round.typst",
    },
  },
  patterns: [
    {
      match: /:/,
      name: "punctuation.separator.colon.typst",
    },
    {
      match: /,/,
      name: "punctuation.separator.comma.typst",
    },
    {
      include: "#code-expr",
    },
  ],
};

const funcCall = (strict: boolean): textmate.Pattern => {
  return {
    name: "meta.expr.call.typst",
    begin: lookAhead(
      strict
        ? new RegExp(/(\.)?/.source + IDENTIFIER.source + /(?=\(|\[)/.source)
        : new RegExp(
            /(\.\s*)?/.source + IDENTIFIER.source + /\s*(?=\(|\[)/.source
          )
    ),
    end: /(?<=\)|\])(?!\[|\()/,
    patterns: [
      // todo: comments?
      //   {
      //     include: "#comments",
      //   },
      {
        match: /\./,
        name: "keyword.operator.accessor.typst",
      },
      {
        match: IDENTIFIER,
        name: "entity.name.function.typst",
      },
      {
        include: "#callArgs",
      },
      {
        include: "#continuousContentBlock",
      },
    ],
  };
};

// https://github.com/microsoft/vscode-textmate/blob/main/test-cases/themes/syntaxes/TypeScript.tmLanguage.json
const arrowFunc: textmate.Pattern = {
  name: "meta.expr.arrow-function.typst",
  patterns: [
    {
      match: new RegExp(`(${IDENTIFIER.source})` + /\s*(?==>)/.source),
      captures: {
        "1": {
          name: "variable.parameter.typst",
        },
      },
    },
    {
      begin: braceMatch(lookAhead(new RegExp(PAREN_BLOCK + /\s*=>/.source))),
      end: /(?==>)/,
      patterns: [
        {
          include: "#comments",
        },
        {
          begin: /\(/,
          end: /\)/,
          beginCaptures: {
            "0": {
              name: "meta.brace.round.typst",
            },
          },
          endCaptures: {
            "0": {
              name: "meta.brace.round.typst",
            },
          },
          patterns: [
            {
              include: "#code-params",
            },
          ],
        },
      ],
    },
    {
      begin: /=>/,
      end: /(?<=\})|(?:(?!\{)(?=\S))/,
      beginCaptures: {
        "0": {
          name: "storage.type.function.arrow.typst",
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
    },
  ],
};

export const typst: textmate.Grammar = {
  repository: {
    primitiveTypes,
    identifier,
    ...strictIf().repository,
    ...strictFor().repository,
    ...strictWhile().repository,
    ...setStatement().repository,
    strictFuncCall: funcCall(true),
    funcCall: funcCall(false),
    callArgs,
    continuousCodeBlock,
    continuousContentBlock,
    arrowFunc,
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
