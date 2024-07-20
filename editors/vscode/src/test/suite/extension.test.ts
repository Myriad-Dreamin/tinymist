import * as assert from "assert";

// You can import and use all API from the 'vscode' module
// as well as import your extension to test it
import * as vscode from "vscode";
import * as myExtension from "../../extension";
import type { Context } from ".";

export async function getTests(ctx: Context) {
    await ctx.suite("Extension Test Suite", (suite) => {
        vscode.window.showInformationMessage("Start all tests.");
        console.log("Start all tests.", vscode.workspace);

        suite.addTest("Sample test", async () => {
            assert.strictEqual([1, 2, 3].indexOf(5), -1);
            assert.strictEqual([1, 2, 3].indexOf(0), -1);
        });
    });
}
