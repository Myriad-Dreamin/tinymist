import { parse, Stylesheet, Rule, Media } from "css";
import { flatten } from "./arrayUtils";

export interface CSSTextsParseResult {
  styleSheets: Stylesheet[];
  unparsable: string[];
}

export function parseCssTexts(
  cssTexts: string[] | Thenable<string[]>,
): Thenable<CSSTextsParseResult> {
  const initialValue = {
    styleSheets: <Stylesheet[]>[],
    unparsable: <string[]>[],
  };

  return Promise.resolve(cssTexts).then((cssTexts) =>
    cssTexts.reduce((acc, cssText) => {
      try {
        acc.styleSheets.push(parse(cssText));
      } catch (error) {
        acc.unparsable.push(cssText);
      }
      return acc;
    }, initialValue),
  );
}

export function getCSSRules(styleSheets: Stylesheet[] | Thenable<Stylesheet[]>): Thenable<Rule[]> {
  return Promise.resolve(styleSheets).then((styleSheets) =>
    styleSheets.reduce((acc, styleSheet) => {
      return acc.concat(findRootRules(styleSheet), findMediaRules(styleSheet));
    }, [] as Rule[]),
  );
}

export function getCSSSelectors(rules: Rule[] | Thenable<Rule[]>): Thenable<string[]> {
  return Promise.resolve(rules).then((rules) => {
    if (rules.length > 0) {
      return flatten(rules.map((rule) => rule.selectors!)).filter(
        (value) => value && value.length > 0,
      );
    } else {
      return [];
    }
  });
}

export function getCSSClasses(selectors: string[] | Thenable<string[]>): Thenable<string[]> {
  return Promise.resolve(selectors).then((selectors) =>
    selectors.reduce((acc, selector) => {
      const className = findClassName(selector);

      if (className && className.length > 0) {
        acc.push(sanitizeClassName(className));
      }

      return acc;
    }, [] as string[]),
  );
}

export function findRootRules(cssAST: Stylesheet): Rule[] {
  // @ts-ignore
  return cssAST.stylesheet!.rules.filter((node) => (<Rule>node).type === "rule");
}

export function findMediaRules(cssAST: Stylesheet): Rule[] {
  let mediaNodes = <Rule[]>cssAST.stylesheet!.rules.filter((node) => {
    // @ts-ignore
    return (<Rule>node).type === "media";
  });
  if (mediaNodes.length > 0) {
    // @ts-ignore
    return flatten(mediaNodes.map((node) => (<Media>node).rules!));
  } else {
    return [];
  }
}

export function findClassName(selector: string): string {
  let classNameStartIndex = selector.lastIndexOf(".");
  if (classNameStartIndex >= 0) {
    let classText = selector.substr(classNameStartIndex + 1);
    // Search for one of ' ', '[', ':' or '>', that isn't escaped with a backslash
    let classNameEndIndex = classText.search(/[^\\][\s\[:>]/);
    if (classNameEndIndex >= 0) {
      return classText.substr(0, classNameEndIndex + 1);
    } else {
      return classText;
    }
  } else {
    return "";
  }
}

export function sanitizeClassName(className: string): string {
  return className.replace(/\\[!"#$%&'()*+,\-./:;<=>?@[\\\]^`{|}~]/g, (substr, ...args) => {
    if (args.length === 2) {
      return substr.slice(1);
    } else {
      return substr;
    }
  });
}
