import { expect, test } from "vitest";
import { wordPattern } from "./language.js";

test("wordPattern", () => {
  const wp = new RegExp(wordPattern.source, "g");
  const words = (str: string) => Array.from(str.matchAll(wp)).map((m) => m[0]);
  expect(words("foo bar baz")).toMatchSnapshot();
  expect(words("-1, -A")).toMatchSnapshot();
  expect(words("A-B")).toMatchSnapshot();
  expect(words("#A-B")).toMatchSnapshot();
  expect(words("#let x = a-b")).toMatchSnapshot();
  expect(words("= Some quick-tests")).toMatchSnapshot();
  expect(words("= Some show-tests")).toMatchSnapshot();
});
