import * as textmate from "./textmate.mjs";
import {
  blockRaw,
  blockRawGeneral,
  blockRawLangs,
  inlineRaw,
} from "./fenced.mjs";

import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "node:url";

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
const exprEndReg =
  /(?<!(?:if|and|or|not|in|!=|==|<=|>=|<|>|\+|-|\*|\/|=|\+=|-=|\*=|\/=)\s*)(?=[\[\{\n])|(?=[;\}\]\)\n]|$)/;

// todo: This is invocable
const codeBlock: textmate.Pattern = {
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

const contentBlock: textmate.Pattern = {
  // name: "meta.block.content.typst",
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
    {
      include: "#markupBrace",
    },
  ],
};

const primitiveColors: textmate.Pattern = {
  match:
    /\b(red|blue|green|black|white|gray|silver|eastern|navy|aqua|teal|purple|fuchsia|maroon|orange|yellow|olive|lime|ltr|rtl|ttb|btt|start|left|center|right|end|top|horizon|bottom)\b(?!-)/,
  name: "support.type.builtin.typst",
};

const primitiveFunctions = {
  match: /\b(?:luma|oklab|oklch|rgb|cmyk|range)\b(?!-)/,
  name: "support.function.builtin.typst",
};

const primitiveTypes: textmate.PatternMatch = {
  match:
    /\b(any|str|int|float|bool|length|content|array|dictionary|arguments)\b(?!-)/,
  name: "entity.name.type.primitive.typst",
};

const IDENTIFIER = /(?<!\)|\]|\})\b[\p{XID_Start}_][\p{XID_Continue}_\-]*/u;

// todo: distinguish type and variable
const identifier: textmate.PatternMatch = {
  match: IDENTIFIER,
  name: "variable.other.readwrite.typst",
};

const mathIdentifier: textmate.PatternMatch = {
  match: /\p{XID_Start}(?:\p{XID_Continue}(?!-))*/u,
  name: "variable.other.readwrite.typst",
};

const markupLabel: textmate.PatternMatch = {
  name: "entity.other.label.typst",
  match: /<[\p{XID_Start}_][\p{XID_Continue}_\-\.:]*>/u,
};

const markupReference: textmate.PatternMatch = {
  name: "entity.other.reference.typst",
  match:
    /(@)[\p{XID_Start}_](?:[\p{XID_Continue}_\-]|[\.:](?!:\s|$|([\.:]*[^\p{XID_Continue}_\-\.:])))*/u,
  captures: {
    "1": {
      name: "punctuation.definition.reference.typst",
    },
  },
};

const markupEscape: textmate.PatternMatch = {
  name: "constant.character.escape.content.typst",
  match: /\\(?:[^u]|u\{?[0-9a-zA-Z]*\}?)/,
};

const markupBrace: textmate.PatternMatch = {
  name: "markup.content.brace.typst",
  match: /[{}()\[\]]/,
};

const stringLiteral: textmate.PatternBeginEnd = {
  name: "string.quoted.double.typst",
  begin: /"/,
  end: /"/,
  beginCaptures: {
    "0": {
      name: "punctuation.definition.string.typst",
    },
  },
  endCaptures: {
    "0": {
      name: "punctuation.definition.string.typst",
    },
  },
  patterns: [
    {
      match: /(\\(?:[^u]|u\{?[0-9a-zA-Z]*\}?))|[^\\"]+/,
      captures: {
        "1": {
          name: "constant.character.escape.string.typst",
        },
      },
    },
  ],
};

// todo: math mode
const markupMath: textmate.Pattern = {
  name: "markup.math.typst",
  begin: /\$/,
  end: /\$/,
  beginCaptures: {
    "0": {
      name: "punctuation.definition.string.math.typst",
    },
  },
  endCaptures: {
    "0": {
      name: "punctuation.definition.string.math.typst",
    },
  },
  patterns: [
    {
      include: "#markupEscape",
    },
    {
      include: "#stringLiteral",
    },
    // todo: correctly parse math identifier
    // {
    //   include: "#mathIdentifier",
    // },
    {
      include: "#markupEnterCode",
    },
  ],
};

const markupHeading: textmate.Pattern = {
  name: "markup.heading.typst",
  begin: /^\s*(=+)(?:(?=[\r\n]|$)|\s+)/,
  end: /\n|(?=<)/,
  beginCaptures: {
    "1": {
      name: "punctuation.definition.heading.typst",
    },
  },
  patterns: [
    {
      include: "#markup",
    },
  ],
};

const common: textmate.Pattern = {
  patterns: [
    {
      include: "#strictComments",
    },
    {
      include: "#blockRaw",
    },
    {
      include: "#inlineRaw",
    },
  ],
};

// These two markup are buggy
const ENABLE_BOLD_ITALIC = false;
const boldItalicMarkup = ENABLE_BOLD_ITALIC
  ? [
      {
        include: "#markupBold",
      },
      {
        include: "#markupItalic",
      },
    ]
  : [];

const markup: textmate.Pattern = {
  patterns: [
    {
      include: "#common",
    },
    {
      include: "#markupEnterCode",
    },
    {
      include: "#markupEscape",
    },
    {
      name: "punctuation.definition.linebreak.typst",
      match: /\\/,
    },
    {
      name: "punctuation.definition.nonbreaking-space.typst",
      match: /\~/,
    },
    {
      name: "punctuation.definition.shy.typst",
      match: /-\?/,
    },
    {
      name: "punctuation.definition.em-dash.typst",
      match: /---/,
    },
    {
      name: "punctuation.definition.en-dash.typst",
      match: /--/,
    },
    {
      name: "punctuation.definition.ellipsis.typst",
      match: /\.\.\./,
    },
    // what is it?
    // {
    //   name: "constant.symbol.typst",
    //   match: /:([a-zA-Z0-9]+:)+/,
    // },
    ...boldItalicMarkup,
    {
      name: "markup.underline.link.typst",
      match: /https?:\/\/[0-9a-zA-Z~\/%#&='',;\.\+\?\-\_]*/,
    },
    {
      include: "#markupMath",
    },
    {
      include: "#markupHeading",
    },
    {
      name: "punctuation.definition.list.unnumbered.typst",
      match: /^\s*-\s+/,
    },
    {
      name: "punctuation.definition.list.numbered.typst",
      match: /^\s*([0-9]+\.|\+)\s+/,
    },
    {
      match: /^\s*(\/)\s+([^:]*)(:)/,
      captures: {
        "1": {
          name: "punctuation.definition.list.description.typst",
        },
        "2": {
          patterns: [
            {
              include: "#markup",
            },
          ],
        },
        "3": {
          name: "markup.list.term.typst",
        },
      },
    },
    {
      include: "#markupLabel",
    },
    {
      include: "#markupReference",
    },
  ],
};

const enterExpression = (kind: string, seek: RegExp): textmate.Pattern => {
  return {
    /// name: 'markup.expr.typst'
    begin: new RegExp("#" + seek.source),
    // `?=(?<![\d#])\.[^\p{XID_Start}_]`: This means that we are on a dot and the next character is not a valid identifier start, but we are not at the beginning of hash or number
    end: /(?<=;)|(?<=[\)\]\}])(?![;\(\[\$])|(?<!#)(?=")|(?=\.(?:[^0-9\p{XID_Start}_]|$))|(?=[\s\}\]\)\$]|$)|(;)/u
      .source,
    beginCaptures: {
      "0": {
        name: kind,
      },
    },
    endCaptures: {
      "1": {
        name: "punctuation.terminator.statement.typst",
      },
    },
    patterns: [
      {
        include: "#expression",
      },
    ],
  };
};

const markupEnterCode: textmate.Pattern = {
  patterns: [
    /// hash and follows a space
    {
      match: /(#)\s/,
      captures: {
        "1": {
          name: "punctuation.definition.hash.typst",
        },
      },
    },
    /// hash and follows a empty
    {
      match: /(#)(;)/,
      captures: {
        "1": {
          name: "punctuation.definition.hash.typst",
        },
        "2": {
          name: "punctuation.terminator.statement.typst",
        },
      },
    },
    enterExpression(
      "keyword.control.hash.typst",
      /(?=(?:break|continue|and|or|not|return|as|in|include|import|let|else|if|for|while|context|set|show)\b(?!-))/
    ),
    enterExpression(
      "entity.name.type.primitive.hash.typst",
      /(?=(?:any|str|int|float|bool|length|content|array|dictionary|arguments)\b(?!-))/
    ),
    enterExpression("keyword.other.none.hash.typst", /(?=(?:none)\b(?!-))/),
    enterExpression(
      "constant.language.boolean.hash.typst",
      /(?=(?:false|true)\b(?!-))/
    ),
    enterExpression(
      "entity.name.function.hash.typst",
      /(?=[\p{XID_Start}_][\p{XID_Continue}_\-]*[\(\[])/u
    ),
    enterExpression(
      "variable.other.readwrite.hash.typst",
      /(?=[\p{XID_Start}_])/u
    ),
    enterExpression("string.hash.hash.typst", /(?=\")/),
    enterExpression("constant.numeric.hash.typst", /(?=\d|\.\d)/),
    enterExpression("keyword.control.hash.typst", new RegExp("")),
  ],
};

const code: textmate.Pattern = {
  patterns: [
    {
      include: "#common",
    },
    {
      include: "#comments",
    },
    {
      name: "punctuation.separator.colon.typst",
      match: /;/,
    },
    {
      include: "#expression",
    },
  ],
};

const FLOAT_OR_INT = /(?:\d+\.(?!\d)|\d*\.?\d+(?:[eE][+-]?\d+)?)/;

const floatUnit = (unit: RegExp, canDotSuff: boolean) =>
  new RegExp(
    FLOAT_OR_INT.source + (canDotSuff ? "" : "(?<!\\.)") + unit.source
  );

const keywordConstants: textmate.Pattern = {
  patterns: [
    {
      name: "keyword.other.none.typst",
      match: /(?<!\)|\]|\})\bnone\b(?!-)/,
    },
    {
      name: "keyword.other.auto.typst",
      match: /(?<!\)|\]|\})\bauto\b(?!-)/,
    },
    {
      name: "constant.language.boolean.typst",
      match: /(?<!\)|\]|\})\b(false|true)\b(?!-)/,
    },
  ],
};

const constants: textmate.Pattern = {
  patterns: [
    {
      name: "constant.numeric.length.typst",
      match: floatUnit(/(mm|pt|cm|in|em)($|\b)/, false),
    },
    {
      name: "constant.numeric.angle.typst",
      match: floatUnit(/(rad|deg)($|\b)/, false),
    },
    {
      name: "constant.numeric.percentage.typst",
      match: floatUnit(/%/, true),
    },
    {
      name: "constant.numeric.fr.typst",
      match: floatUnit(/fr/, false),
    },
    {
      name: "constant.numeric.integer.typst",
      match:
        /(?<!\)|\]|\})(^|(?<=\s|#)|\b)\d+\b(?!\.(?:[^\p{XID_Start}_]|$)|[eE])/u,
    },
    {
      name: "constant.numeric.hex.typst",
      match: /(?<!\)|\]|\})(^|(?<=\s|#)|\b)0x[0-9a-fA-F]+\b/,
    },
    {
      name: "constant.numeric.octal.typst",
      match: /(?<!\)|\]|\})(^|(?<=\s|#)|\b)0o[0-7]+\b/,
    },
    {
      name: "constant.numeric.binary.typst",
      match: /(?<!\)|\]|\})(^|(?<=\s|#)|\b)0b[01]+\b/,
    },
    {
      name: "constant.numeric.float.typst",
      match: floatUnit(new RegExp(""), true),
    },
    {
      include: "#stringLiteral",
    },
    {
      include: "#markupMath",
    },
  ],
};

const expressions = (): textmate.Grammar => {
  const expression: textmate.Pattern = {
    patterns: [
      { include: "#comments" },
      { include: "#arrowFunc" },
      { include: "#arrayOrDict" },
      { include: "#contentBlock" },
      {
        match: /\b(break|continue)\b(?!-)/,
        name: "keyword.control.loop.typst",
      },
      {
        match: /\b(in)\b(?!-)/,
        name: "keyword.operator.range.typst",
      },
      {
        match: /\b(and|or|not)\b(?!-)/,
        name: "keyword.other.logical.typst",
      },
      {
        match: /\b(return)\b(?!-)/,
        name: "keyword.control.flow.typst",
      },
      { include: "#markupLabel" },
      { include: "#blockRaw" },
      { include: "#inlineRaw" },
      { include: "#codeBlock" },
      { include: "#letStatement" },
      { include: "#showStatement" },
      { include: "#contextStatement" },
      { include: "#setStatement" },
      { include: "#forStatement" },
      { include: "#whileStatement" },
      { include: "#ifStatement" },
      { include: "#importStatement" },
      { include: "#includeStatement" },
      { include: "#strictFuncCallOrPropAccess" },
      { include: "#primitiveColors" },
      { include: "#primitiveFunctions" },
      { include: "#primitiveTypes" },
      // todo: enable if only if for completely right grammar
      { include: "#keywordConstants" },
      { include: "#identifier" },
      { include: "#constants" },
      {
        match: /(as)\b(?!-)/,
        name: "keyword.control.typst",
      },
      {
        match: /(in)\b(?!-)/,
        name: "keyword.operator.range.typst",
      },
      {
        match: /\.\./,
        name: "keyword.operator.spread.typst",
      },
      {
        match: /:/,
        name: "punctuation.separator.colon.typst",
      },
      {
        match: /\./,
        name: "keyword.operator.accessor.typst",
      },
      {
        match:
          /\+|\\|\/|(?<![[:alpha:]])(?<!\w)(?<!\d)-(?![[:alnum:]-][[:alpha:]_])/,
        name: "keyword.operator.arithmetic.typst",
      },
      {
        match: /==|!=|<=|<|>=|>/,
        name: "keyword.operator.relational.typst",
      },
      {
        begin: /(\+=|-=|\*=|\/=|=)/,
        end: /(?=[\n;\)\]\}])/,
        beginCaptures: {
          "1": {
            name: "keyword.operator.assignment.typst",
          },
        },
        patterns: [
          {
            include: "#expression",
          },
        ],
      },
    ],
  };

  const arrayOrDict: textmate.Pattern = {
    patterns: [
      /// empty array ()
      {
        match: /(\()\s*(\))/,
        captures: {
          "1": {
            name: "meta.brace.round.typst",
          },
          "2": {
            name: "meta.brace.round.typst",
          },
        },
      },
      /// empty dictionary (:)
      {
        match: /(\()\s*(:)\s*(\))/,
        captures: {
          "1": {
            name: "meta.brace.round.typst",
          },
          "2": {
            name: "punctuation.separator.colon.typst",
          },
          "3": {
            name: "meta.brace.round.typst",
          },
        },
      },
      /// parentheisized expressions: (...)
      // todo: This is invocable
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
            include: "#literalContent",
          },
        ],
      },
    ],
  };

  const literalContent: textmate.Pattern = {
    patterns: [
      {
        name: "punctuation.separator.colon.typst",
        match: /:/,
      },
      {
        name: "punctuation.separator.comma.typst",
        match: /,/,
      },
      {
        include: "#expression",
      },
    ],
  };

  return {
    repository: {
      expression,
      arrayOrDict,
      literalContent,
    },
  };
};

const blockComment: textmate.Pattern = {
  name: "comment.block.typst",
  begin: /\/\*/,
  end: /\*\//,
  beginCaptures: {
    "0": {
      name: "punctuation.definition.comment.typst",
    },
  },
  patterns: [
    {
      include: "#blockComment",
    },
  ],
};

const lineCommentInner = (strict: boolean): textmate.Pattern => {
  return {
    name: "comment.line.double-slash.typst",
    begin: strict ? /(?<!:)\/\// : /\/\//,
    end: /(?=$|\n)/,
    beginCaptures: {
      "0": {
        name: "punctuation.definition.comment.typst",
      },
    },
  };
};

const strictLineComment = lineCommentInner(true);
const lineComment = lineCommentInner(false);

const strictComments: textmate.Pattern = {
  patterns: [{ include: "#blockComment" }, { include: "#strictLineComment" }],
};

const comments: textmate.Pattern = {
  patterns: [{ include: "#blockComment" }, { include: "#lineComment" }],
};

const markupAnnotate = (ch: string, style: string): textmate.Pattern => {
  const MARKUP_BOUNDARY = `[\\W\\p{Han}\\p{Hangul}\\p{Katakana}\\p{Hiragana}]`;
  const notationAtBound = `(?:(^${ch}|${ch}$|((?<=${MARKUP_BOUNDARY})${ch})|(${ch}(?=${MARKUP_BOUNDARY}))))`;
  return {
    name: `markup.${style}.typst`,
    begin: notationAtBound,
    end: new RegExp(notationAtBound + `|\\n|(?=\\])`),
    beginCaptures: {
      "0": {
        name: `punctuation.definition.${style}.typst`,
      },
    },
    endCaptures: {
      "0": {
        name: `punctuation.definition.${style}.typst`,
      },
    },
    patterns: [
      {
        include: "#markup",
      },
    ],
  };
};

const markupBold = markupAnnotate("\\*", "bold");
const markupItalic = markupAnnotate("_", "italic");

const includeStatement: textmate.Pattern = {
  name: "meta.expr.include.typst",
  begin: /(\binclude\b(?!-))\s*/,
  end: /(?=[\n;\}\]\)])/,
  beginCaptures: {
    "1": {
      name: "keyword.control.import.typst",
    },
  },
  patterns: [
    {
      include: "#comments",
    },
    {
      include: "#expression",
    },
  ],
};

// todo: sometimes eat a character
const importStatement = (): textmate.Grammar => {
  const importStatement: textmate.Pattern = {
    name: "meta.expr.import.typst",
    begin: /(\bimport\b(?!-))\s*/,
    end: /(?=[\n;\}\]\)])/,
    beginCaptures: {
      "1": {
        name: "keyword.control.import.typst",
      },
    },
    patterns: [
      {
        include: "#comments",
      },
      {
        include: "#importPathClause",
      },
      {
        match: /\:/,
        name: "punctuation.separator.colon.typst",
      },
      {
        match: /\*/,
        name: "keyword.operator.wildcard.typst",
      },
      {
        match: /\,/,
        name: "punctuation.separator.comma.typst",
      },
      {
        include: "#importAsClause",
      },
      {
        include: "#expression",
      },
    ],
  };

  /// import expression until as|:
  const importPathClause: textmate.Pattern = {
    begin: /(\bimport\b(?!-))\s*/,
    // todo import as
    end: /(?=\:|as)/,
    beginCaptures: {
      "1": {
        name: "keyword.control.import.typst",
      },
    },
    patterns: [
      {
        include: "#comments",
      },
      {
        include: "#expression",
      },
    ],
  };

  /// as expression
  const importAsClause: textmate.Pattern = {
    // todo: as...
    begin: /(\bas\b)\s*/,
    end: /(?=[\s;\}\]\)])/,
    beginCaptures: {
      "1": {
        name: "keyword.control.import.typst",
      },
    },
    patterns: [
      {
        include: "#comments",
      },
      {
        include: "#identifier",
      },
    ],
  };

  return {
    repository: {
      importStatement,
      importPathClause,
      importAsClause,
    },
  };
};

const letStatement = (): textmate.Grammar => {
  const letStatement: textmate.Pattern = {
    name: "meta.expr.let.typst",
    begin: lookAhead(/(let\b(?!-))/),
    end: /(?!\()(?=[\s;\}\]\)])/,
    patterns: [
      /// Matches any comments
      {
        include: "#comments",
      },
      /// Matches binding clause
      {
        include: "#letBindingClause",
      },
      /// Matches init assignment clause
      {
        include: "#letInitClause",
      },
    ],
  };

  const letBindingClause: textmate.Pattern = {
    // name: "meta.let.binding.typst",
    begin: /(let\b(?!-))\s*/,
    end: /(?=[=;\]}\n])/,
    beginCaptures: {
      "1": {
        name: "storage.type.typst",
      },
    },
    patterns: [
      {
        include: "#comments",
      },
      /// Matches a func call after the set clause
      {
        begin: /(\b[\p{XID_Start}_][\p{XID_Continue}_\-]*)(\()/u,
        end: /\)/,
        beginCaptures: {
          "1": {
            name: "entity.name.function.typst",
            patterns: [
              {
                include: "#primitiveFunctions",
              },
            ],
          },
          "2": {
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
            include: "#patternOrArgsBody",
          },
        ],
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
        patterns: [{ include: "#patternOrArgsBody" }],
      },
      {
        include: "#identifier",
      },
    ],
  };

  const letInitClause: textmate.Pattern = {
    // name: "meta.let.init.typst",
    begin: /=\s*/,
    end: /(?<!\s*=)(?=[;\]})\n])/,
    beginCaptures: {
      "0": {
        name: "keyword.operator.assignment.typst",
      },
    },
    patterns: [
      {
        include: "#comments",
      },
      {
        include: "#expression",
      },
    ],
  };

  return {
    repository: {
      letStatement,
      letBindingClause,
      letInitClause,
    },
  };
};

/**
 * Matches a (strict grammar) if in markup context.
 */
const ifStatement = (): textmate.Grammar => {
  const ifStatement: textmate.Pattern = {
    name: "meta.expr.if.typst",
    begin: lookAhead(/(else\s+)?(if\b(?!-))/),
    end: /(?<=\}|\])(?!\s*(else)\b(?!-)|[\[\{])|(?<=else)(?!\s*(?:if\b(?!-)|[\[\{]))|(?=[;\}\]\)\n]|$)/,
    patterns: [
      { include: "#comments" },
      { include: "#ifClause" },
      { include: "#elseClause" },
      { include: "#codeBlock" },
      { include: "#contentBlock" },
    ],
  };

  const ifClause: textmate.Pattern = {
    //   name: "meta.if.clause.typst",
    begin: /\bif\b(?!-)/,
    end: exprEndReg,
    beginCaptures: {
      "0": {
        name: "keyword.control.conditional.typst",
      },
    },
    patterns: [{ include: "#expression" }],
  };

  const elseClause: textmate.Pattern = {
    match: /\belse\b(?!-)/,
    name: "keyword.control.conditional.typst",
  };

  return {
    repository: {
      ifStatement,
      ifClause,
      elseClause,
    },
  };
};

const forStatement = (): textmate.Grammar => {
  // for v in expr { ... }
  const forStatement: textmate.Pattern = {
    name: "meta.expr.for.typst",
    begin: lookAhead(/(for\b(?!-))\s*/),
    end: /(?<=[\}\]])(?![\{\[])|(?=[;\}\]\)\n]|$)/,
    patterns: [
      { include: "#comments" },
      { include: "#forClause" },
      { include: "#codeBlock" },
      { include: "#contentBlock" },
    ],
  };

  const forClause: textmate.Pattern = {
    // name: "meta.for.clause.bind.typst",
    begin: /(for\b)\s*/,
    end: exprEndReg,
    beginCaptures: {
      "1": {
        name: "keyword.control.loop.typst",
      },
    },
    patterns: [{ include: "#expression" }],
  };

  return {
    repository: {
      forStatement,
      forClause,
    },
  };
};

const whileStatement = (): textmate.Grammar => {
  // for v in expr { ... }
  const whileStatement: textmate.Pattern = {
    name: "meta.expr.while.typst",
    begin: lookAhead(/(while\b(?!-))/),
    end: /(?<=[\}\]])(?![\{\[])|(?=[;\}\]\)\n]|$)/,
    patterns: [
      { include: "#comments" },
      { include: "#whileClause" },
      { include: "#codeBlock" },
      { include: "#contentBlock" },
    ],
  };

  const whileClause: textmate.Pattern = {
    // name: "meta.while.clause.bind.typst",
    begin: /(while\b)\s*/,
    end: exprEndReg,
    beginCaptures: {
      "1": {
        name: "keyword.control.loop.typst",
      },
    },
    patterns: [{ include: "#expression" }],
  };

  return {
    repository: {
      whileStatement,
      whileClause,
    },
  };
};

const contextStatement: textmate.Pattern = {
  name: "meta.expr.context.typst",
  begin: /\bcontext\b(?!-)/,
  end: /(?<=[\}\]])|(?<!\bcontext\s*)(?=[\{\[])|(?=[;\}\]\)\n]|$)/,
  beginCaptures: {
    "0": {
      name: "keyword.control.other.typst",
    },
  },
  patterns: [{ include: "#expression" }],
};

const setStatement = (): textmate.Grammar => {
  const setStatement: textmate.Pattern = {
    name: "meta.expr.set.typst",
    begin: lookAhead(new RegExp(/(set\b(?!-))\s*/.source + IDENTIFIER.source)),
    end: /(?<=\))(?!\s*if\b)|(?=[\s;\{\[\}\]\)])/,
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
    end: /(?=if)|(?=[\n;\]}])/,
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
        include: "#strictFuncCallOrPropAccess",
      },
      {
        include: "#identifier",
      },
    ],
  };

  const setIfClause: textmate.Pattern = {
    // name: "meta.set.if.clause.cond.typst",
    begin: /(if\b(?!-))\s*/,
    end: /(?<=\S)(?<!and|or|not|in|!=|==|<=|>=|<|>|\+|-|\*|\/|=|\+=|-=|\*=|\/=)(?!\s*(?:and|or|not|in|!=|==|<=|>=|<|>|\+|-|\*|\/|=|\+=|-=|\*=|\/=|\.))|(?=[\n;\}\]\)])/,
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
        include: "#expression",
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

const showStatement = (): textmate.Grammar => {
  const showStatement: textmate.Pattern = {
    name: "meta.expr.show.typst",
    begin: lookAhead(/(show\b(?!-))/),
    end: /(?=[\s;\{\[\}\]\)])/,
    patterns: [
      /// Matches any comments
      {
        include: "#comments",
      },
      /// Matches show any clause
      {
        include: "#showAnyClause",
      },
      /// Matches select clause
      {
        include: "#showSelectClause",
      },
      /// Matches substitution clause
      {
        include: "#showSubstClause",
      },
    ],
  };

  const showAnyClause: textmate.Pattern = {
    // name: "meta.show.clause.select.typst",
    match: /(show\b)\s*(?=\:)/,
    captures: {
      "1": {
        name: "keyword.control.other.typst",
      },
    },
  };

  const showSelectClause: textmate.Pattern = {
    // name: "meta.show.clause.select.typst",
    begin: /(show\b)\s*/,
    end: /(?=[:;\}\]\n])/,
    beginCaptures: {
      "1": {
        name: "keyword.control.other.typst",
      },
    },
    patterns: [
      {
        include: "#comments",
      },
      {
        include: "#markupLabel",
      },
      /// Matches a func call after the set clause
      {
        include: "#expression",
      },
    ],
  };

  const showSubstClause: textmate.Pattern = {
    // name: "meta.show.clause.subst.typst",
    begin: /(\:)\s*/,
    end: /(?<!:)(?<=\S)(?!\S)|(?=[\n;\}\]\)])/,
    beginCaptures: {
      "1": {
        name: "punctuation.separator.colon.typst",
      },
    },
    patterns: [
      {
        include: "#comments",
      },
      {
        include: "#expression",
      },
    ],
  };

  return {
    repository: {
      showStatement,
      showAnyClause,
      showSelectClause,
      showSubstClause,
    },
  };
};

// todo: { f }(..args)
// todo: ( f )(..args)
const callArgs: textmate.Pattern = {
  //   name: "meta.call.args.typst",
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
  patterns: [{ include: "#patternOrArgsBody" }],
};

const patternOrArgsBody: textmate.Pattern = {
  patterns: [
    { include: "#comments" },
    {
      match: /,/,
      name: "punctuation.separator.comma.typst",
    },
    { include: "#expression" },
  ],
};

const funcCallOrPropAccess = (strict: boolean): textmate.Pattern => {
  return {
    name: "meta.expr.call.typst",
    begin: lookAhead(
      strict
        ? new RegExp(/(\.)?/.source + IDENTIFIER.source + /(?=\(|\[)/.source)
        : new RegExp(
            /(\.\s*)?/.source + IDENTIFIER.source + /\s*(?=\(|\[)/.source
          )
    ),
    end: strict
      ? /(?:(?<=\)|\])(?:(?![\[\(\.])|$))|(?=[\s;\,\}\]\)]|$)/
      : /(?:(?<=\)|\])(?:(?![\[\(\.])|$))|(?=[\n;\,\}\]\)]|$)/,
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
        match: new RegExp(
          IDENTIFIER.source +
            (strict ? /(?=\(|\[)/.source : /\s*(?=\(|\[)/.source)
        ),
        name: "entity.name.function.typst",
        patterns: [
          {
            include: "#primitiveFunctions",
          },
        ],
      },
      {
        include: "#identifier",
      },
      // empty args
      {
        // name: "meta.call.args.typst",
        match: /(\()\s*(\))/,
        captures: {
          "1": {
            name: "meta.brace.round.typst",
          },
          "2": {
            name: "meta.brace.round.typst",
          },
        },
      },
      {
        include: "#callArgs",
      },
      {
        include: "#contentBlock",
      },
    ],
  };
};

// todo: #x => y should be parsed as |#x|=>|y
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
              include: "#patternOrArgsBody",
            },
          ],
        },
      ],
    },
    {
      begin: /=>/,
      end: /(?<=\}|\])|(?:(?!\{|\[)(?=[\n\S;]))/,
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
          include: "#expression",
        },
      ],
    },
  ],
};

export const typst: textmate.Grammar = {
  repository: {
    common,
    markup,
    markupEnterCode,
    code,
    keywordConstants,
    constants,

    primitiveColors,
    primitiveFunctions,
    primitiveTypes,
    identifier,
    mathIdentifier,
    markupLabel,
    markupReference,
    markupEscape,
    stringLiteral,

    comments,
    strictComments,
    blockComment,
    lineComment,
    strictLineComment,

    inlineRaw,
    blockRaw,
    ...blockRawLangs.reduce((acc: Record<string, textmate.Pattern>, lang) => {
      acc[lang.lang.replace(/\./g, "_")] = lang;
      return acc;
    }, {}),
    blockRawGeneral,

    markupBold,
    markupItalic,
    markupMath,
    markupHeading,
    markupBrace,

    ...expressions().repository,

    includeStatement,
    ...importStatement().repository,
    ...letStatement().repository,
    ...ifStatement().repository,
    ...forStatement().repository,
    ...whileStatement().repository,
    contextStatement,
    ...setStatement().repository,
    ...showStatement().repository,
    strictFuncCallOrPropAccess: funcCallOrPropAccess(true),
    // todo: distinguish strict and non-strict for markup and code mode.
    // funcCallOrPropAccess: funcCallOrPropAccess(false),
    callArgs,
    patternOrArgsBody,
    codeBlock,
    contentBlock,
    arrowFunc,
  },
};

function generate() {
  const dirname = fileURLToPath(new URL(".", import.meta.url));

  const typstPath = path.join(dirname, "../typst.tmLanguage");

  const compiled = textmate.compile(typst);
  const repository = JSON.parse(compiled).repository;

  // dump to file
  fs.writeFileSync(
    path.join(dirname, "../typst.tmLanguage.json"),
    JSON.stringify({
      $schema:
        "https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json",
      scopeName: "source.typst",
      name: "typst",
      patterns: [
        {
          include: "#markup",
        },
      ],
      repository,
    })
  );

  // dump to file
  fs.writeFileSync(
    path.join(dirname, "../typst-code.tmLanguage.json"),
    JSON.stringify({
      $schema:
        "https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json",
      scopeName: "source.typst-code",
      name: "typst-code",
      patterns: [
        {
          include: "#code",
        },
      ],
      repository,
    })
  );
}

// console.log(typst!.repository!.forStatement);
generate();

// todo: this is fixed in v0.11.0
// #code(```typ
//   #let a = 1; #let b = 2;
//   #(a, b) = (4, 5)
//   #a, #b
//   ```)
