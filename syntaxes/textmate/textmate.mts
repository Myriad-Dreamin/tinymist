import { ANNOTATE_META } from "./feature.mjs";

export interface Grammar {
  patterns?: Pattern[];
  repository?: Repository;
}
export type Repository = Record<string, Pattern>;
type MaybeRegExp = RegExp | string;

export type PatternCommon = Pick<PatternAny, "comment" | "disabled" | "name">;
export type PatternInclude = PatternCommon &
  Pick<PatternAny, "include" | "patterns">;
export type PatternMatch = PatternCommon &
  Pick<PatternAny, "match" | "captures">;
export type PatternBeginEnd = PatternCommon &
  Pick<
    PatternAny,
    | "begin"
    | "end"
    | "contentName"
    | "beginCaptures"
    | "endCaptures"
    | "applyEndPatternLast"
    | "patterns"
  >;
export type PatternBeginWhile = PatternCommon &
  Pick<
    PatternAny,
    | "begin"
    | "while"
    | "contentName"
    | "beginCaptures"
    | "whileCaptures"
    | "patterns"
  >;
export type Pattern =
  | PatternInclude
  | PatternMatch
  | PatternBeginEnd
  | PatternBeginWhile;

interface PatternAny {
  /**
   * A comment.
   * @description A comment.
   * @type string
   */
  comment?: string;
  /**
   * Set this property to 1 to disable the current pattern.
   * @description Set this property to 1 to disable the current pattern.
   * @type number
   * @minimum 0
   * @maximum 1
   */
  disabled?: number;
  /**
   * This allows you to reference a different language, recursively reference the grammar itself or a rule declared in this file's repository.
   * @description This allows you to reference a different language, recursively reference the grammar itself or a rule declared in this file's repository.
   * @type string
   */
  include?: string;
  /**
   * A regular expression which is used to identify the portion of text to which the name should be assigned. Example: '\b(true|false)\b'.
   * @description A regular expression which is used to identify the portion of text to which the name should be assigned. Example: '\b(true|false)\b'.
   * @type string
   */
  match?: MaybeRegExp;
  /**
   * The name which gets assigned to the portion matched. This is used for styling and scope-specific settings and actions, which means it should generally be derived from one of the standard names.
   * @description The name which gets assigned to the portion matched. This is used for styling and scope-specific settings and actions, which means it should generally be derived from one of the standard names.
   * @type string
   */
  name?: string;
  /**
   * This key is similar to the name key but only assigns the name to the text between what is matched by the begin/end patterns.
   * @description This key is similar to the name key but only assigns the name to the text between what is matched by the begin/end patterns.
   * @type string
   */
  contentName?: string;
  /**
   * These keys allow matches which span several lines and must both be mutually exclusive with the match key. Each is a regular expression pattern. begin is the pattern that starts the block and end is the pattern which ends the block. Captures from the begin pattern can be referenced in the end pattern by using normal regular expression back-references. This is often used with here-docs. A begin/end rule can have nested patterns using the patterns key.
   * @description These keys allow matches which span several lines and must both be mutually exclusive with the match key. Each is a regular expression pattern. begin is the pattern that starts the block and end is the pattern which ends the block. Captures from the begin pattern can be referenced in the end pattern by using normal regular expression back-references. This is often used with here-docs. A begin/end rule can have nested patterns using the patterns key.
   * @type string
   */
  begin: MaybeRegExp;
  /**
   * These keys allow matches which span several lines and must both be mutually exclusive with the match key. Each is a regular expression pattern. begin is the pattern that starts the block and end is the pattern which ends the block. Captures from the begin pattern can be referenced in the end pattern by using normal regular expression back-references. This is often used with here-docs. A begin/end rule can have nested patterns using the patterns key.
   * @description These keys allow matches which span several lines and must both be mutually exclusive with the match key. Each is a regular expression pattern. begin is the pattern that starts the block and end is the pattern which ends the block. Captures from the begin pattern can be referenced in the end pattern by using normal regular expression back-references. This is often used with here-docs. A begin/end rule can have nested patterns using the patterns key.
   * @type string
   */
  end: MaybeRegExp;
  /**
   * These keys allow matches which span several lines and must both be mutually exclusive with the match key. Each is a regular expression pattern. begin is the pattern that starts the block and while continues it.
   * @description These keys allow matches which span several lines and must both be mutually exclusive with the match key. Each is a regular expression pattern. begin is the pattern that starts the block and while continues it.
   * @type string
   */
  while: MaybeRegExp;
  /**
   * Allows you to assign attributes to the captures of the match pattern. Using the captures key for a begin/end rule is short-hand for giving both beginCaptures and endCaptures with same values.
   * @description Allows you to assign attributes to the captures of the match pattern. Using the captures key for a begin/end rule is short-hand for giving both beginCaptures and endCaptures with same values.
   * @type Captures
   */
  captures?: Captures;
  /**
   * Allows you to assign attributes to the captures of the begin pattern. Using the captures key for a begin/end rule is short-hand for giving both beginCaptures and endCaptures with same values.
   * @description Allows you to assign attributes to the captures of the begin pattern. Using the captures key for a begin/end rule is short-hand for giving both beginCaptures and endCaptures with same values.
   * @type Captures
   */
  beginCaptures?: Captures;
  /**
   * Allows you to assign attributes to the captures of the end pattern. Using the captures key for a begin/end rule is short-hand for giving both beginCaptures and endCaptures with same values.
   * @description Allows you to assign attributes to the captures of the end pattern. Using the captures key for a begin/end rule is short-hand for giving both beginCaptures and endCaptures with same values.
   * @type Captures
   */
  endCaptures?: Captures;
  /**
   * Allows you to assign attributes to the captures of the while pattern. Using the captures key for a begin/while rule is short-hand for giving both beginCaptures and whileCaptures with same values.
   * @description Allows you to assign attributes to the captures of the while pattern. Using the captures key for a begin/while rule is short-hand for giving both beginCaptures and whileCaptures with same values.
   * @type Captures
   */
  whileCaptures?: Captures;
  /**
   * Applies to the region between the begin and end matches.
   * @description Applies to the region between the begin and end matches.
   * @type Pattern[]
   */
  patterns?: Pattern[];
  /**
   * @description
   * @type number
   * @minimum 0
   * @maximum 1
   */
  applyEndPatternLast?: number;
}

export type NumberStrings =
  | "0"
  | "1"
  | "2"
  | "3"
  | "4"
  | "5"
  | "6"
  | "7"
  | "8"
  | "9";

export type Captures = Partial<Record<NumberStrings, Capture>>;
export interface Capture {
  name?: string;
  patterns?: Pattern[];
}

/**
 * A function to create a look-ahead regular expression.
 *
 * @param pattern A regular expression
 * @returns A regular expression that only looks ahead the pattern without consuming it.
 */
export function lookAhead(pattern: RegExp) {
  return new RegExp(`(?=(?:${pattern.source}))`);
}

/**
 * Unions a list of regular expressions into a single regular expression that matches any of the given patterns.
 *
 * @param patterns A list of regular expressions
 * @returns A regular expression that matches any of the patterns.
 */
export function oneOf(...patterns: RegExp[]) {
  return new RegExp(
    patterns
      .map((p) => {
        const src = p.source;
        if (src.startsWith("(")) {
          return src;
        }

        return `(?:${src})`;
      })
      .join("|")
  );
}

/**
 * A function to replace a group in a regular expression with another regular expression by simple string substitution.
 *
 * @param pat A regular expression
 * @param group the substring to be replaced
 * @param replacement A regular expression to replace the group
 */
export function replaceGroup(pat: RegExp, group: string, replacement: RegExp) {
  return new RegExp(pat.source.replace(group, replacement.source), pat.flags);
}

/**
 * A wrapper function to annotate the scope name.
 *
 * @param name A scope name
 * @returns return the scope name if we should annotate the scope name, otherwise return undefined
 */
export function metaName(name: string) {
  return ANNOTATE_META ? name : undefined;
}

export function compile(s: Grammar): string {
  return JSON.stringify(
    s,
    function (_k, v) {
      if (v instanceof RegExp) {
        return v.source;
      }
      return v;
    },
    2
  );
}
