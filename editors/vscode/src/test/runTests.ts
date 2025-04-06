import * as path from "path";
import * as fs from "fs";
import { tmpdir } from "os";

import { runTests } from "@vscode/test-electron";

async function main() {
  // The folder containing the Extension Manifest package.json
  // Passed to `--extensionDevelopmentPath`
  const extensionDevelopmentPath = path.resolve(__dirname, "../../");

  const userDataDirectory = fs.mkdtempSync(path.join(tmpdir(), "vsce"));

  // Minimum supported version.
  const jsonData = fs.readFileSync(path.join(extensionDevelopmentPath, "package.json"));
  const json = JSON.parse(jsonData.toString());
  let minimalVersion: string = json.engines.vscode;
  if (minimalVersion.startsWith("^")) minimalVersion = minimalVersion.slice(1);

  // All e2e test suites (either unit tests or integration tests) should be in subfolders.
  const extensionTestsPath = path.resolve(__dirname, "./e2e/index");

  const launchArgs = [
    "--disable-extensions",

    `--user-data-dir=${userDataDirectory}`,
    // https://github.com/microsoft/vscode/issues/184687
    // https://github.com/sourcegraph/sourcegraph/blob/341bfe749f4660709f9fde2d7228a361177a3a45/client/vscode/tests/launch.ts
    "--no-sandbox",
  ];

  const extensionTestsEnv: Record<string, any> = {};

  // set filter
  const filter = process.argv[2];
  if (filter) {
    extensionTestsEnv.VSCODE_TEST_FILTER = filter;
  }

  // Run tests using the minimal supported version and the latest one.
  for (const version of [minimalVersion, "stable"]) {
    for (const uri of [
      path.resolve(extensionDevelopmentPath, "e2e-workspaces/export"),
      undefined,
    ]) {
      await runTests({
        version,
        launchArgs: uri ? [...launchArgs, uri] : launchArgs,
        extensionDevelopmentPath,
        extensionTestsPath,
        extensionTestsEnv,
      });
    }
  }

  // await runVSCodeCommand(["--install-extension", "ms-python.python"], { version: "1.60.0" });
}

main().catch((err) => {
  // eslint-disable-next-line no-console
  console.error("Failed to run tests", err);
  process.exit(1);
});
