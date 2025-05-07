import { expect, test } from "vitest";
import { machineChanges, mirrorLogRe, wordPattern } from "./language.js";

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

test("machineChanges", () => {
  expect(machineChanges.test("/.git/HEAD")).toBe(true);
  expect(machineChanges.test("/target/debug/tinymist")).toBe(true);
  expect(machineChanges.test("/.git\\HEAD")).toBe(true);
  expect(machineChanges.test("/target\\debug\\tinymist")).toBe(true);
  expect(machineChanges.test("a/.git/HEAD")).toBe(true);
  expect(machineChanges.test("a/target/debug/tinymist")).toBe(true);
  expect(machineChanges.test("a/.git\\HEAD")).toBe(true);
  expect(machineChanges.test("a/target\\debug\\tinymist")).toBe(true);
  expect(machineChanges.test("/node_modules/test.js")).toBe(true);
  expect(machineChanges.test("/main.png")).toBe(false);
  expect(machineChanges.test("/main.log")).toBe(false);
  expect(mirrorLogRe.test("/main.js")).toBe(false);
  expect(mirrorLogRe.test("/main.log")).toBe(false);
  expect(machineChanges.test("/my-target/debug/tinymist")).toBe(false);
});

test("mirrorLogRe", () => {
  expect(mirrorLogRe.test("tinymist-dap.log")).toBe(true);
  expect(mirrorLogRe.test("tinymist-lsp.log")).toBe(true);
  expect(mirrorLogRe.test("main.typ")).toBe(false);
  expect(mirrorLogRe.test("main.png")).toBe(false);
  expect(mirrorLogRe.test("main.log")).toBe(false);
});
