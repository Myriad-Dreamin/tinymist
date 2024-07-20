import * as path from "path";
import * as cp from "child_process";
import * as fs from "fs";

import { runTests } from "@vscode/test-electron";

async function main() {
    // The folder containing the Extension Manifest package.json
    // Passed to `--extensionDevelopmentPath`
    const extensionDevelopmentPath = path.resolve(__dirname, "../../");

    // Minimum supported version.
    const jsonData = fs.readFileSync(path.join(extensionDevelopmentPath, "package.json"));
    const json = JSON.parse(jsonData.toString());
    let minimalVersion: string = json.engines.vscode;
    if (minimalVersion.startsWith("^")) minimalVersion = minimalVersion.slice(1);

    // All test suites (either unit tests or integration tests) should be in subfolders.
    const extensionTestsPath = path.resolve(__dirname, "./suite/index");

    const launchArgs = ["--disable-extensions"];

    // Run tests using the minimal supported version.
    await runTests({
        version: minimalVersion,
        launchArgs,
        extensionDevelopmentPath,
        extensionTestsPath,
    });

    // and the latest one
    await runTests({
        version: "stable",
        launchArgs,
        extensionDevelopmentPath,
        extensionTestsPath,
    });

    // await runVSCodeCommand(["--install-extension", "ms-python.python"], { version: "1.60.0" });
}

main().catch((err) => {
    // eslint-disable-next-line no-console
    console.error("Failed to run tests", err);
    process.exit(1);
});
