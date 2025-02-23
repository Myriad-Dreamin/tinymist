import { workspace, window } from "vscode";
// @ts-ignore
import flow from "lodash.flow";
import uriFilesReader from "./uriFilesReader";
import { distinct, distinctByXXHash } from "./arrayUtils";
import { parseCssTexts, getCSSRules, getCSSSelectors, getCSSClasses } from "./cssUtils";

const styleSheetsReader = flow(uriFilesReader, distinctByXXHash, parseCssTexts);
const distinctCSSClassesExtractor = flow(getCSSRules, getCSSSelectors, getCSSClasses, distinct);

export default function (): Thenable<string[]> {
  const startTime = process.hrtime();

  return styleSheetsReader(
    workspace.findFiles("**/*.css", ""),
    workspace.getConfiguration("files").get("encoding", "utf8"),
  ).then((parseResult: any) => {
    return distinctCSSClassesExtractor(parseResult.styleSheets).then((distinctCssClasses: any) => {
      const elapsedTime = process.hrtime(startTime);

      console.log(`Elapsed time: ${elapsedTime[0]} s ${Math.trunc(elapsedTime[1] / 1e6)} ms`);
      console.log(`Files processed: ${parseResult.styleSheets.length}`);
      console.log(`Skipped due to parse errors: ${parseResult.unparsable.length}`);
      console.log(`CSS classes discovered: ${distinctCssClasses.length}`);

      window.setStatusBarMessage(
        `HTML Class Suggestions processed ${parseResult.styleSheets.length} distinct css files and discovered ${distinctCssClasses.length} css classes.`,
        10000,
      );

      return distinctCssClasses;
    });
  });
}
