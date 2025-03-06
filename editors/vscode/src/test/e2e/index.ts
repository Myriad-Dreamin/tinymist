// Inspired from https://github.com/camel-tooling/camel-lsp-client-vscode/blob/main/src/test/suite/completion.util.ts

import * as assert from "node:assert/strict";
import { readdir } from "fs/promises";
import * as path from "path";
import * as vscode from "vscode";
import * as fs from "fs";

class Test {
  readonly name: string;
  readonly f: () => Promise<void>;

  constructor(name: string, f: () => Promise<void>) {
    this.name = name;
    this.f = f;
  }
}

class Suite {
  tests: Test[];

  constructor() {
    this.tests = [];
  }

  public addTest(name: string, f: () => Promise<void>): void {
    const test = new Test(name, f);
    this.tests.push(test);
  }

  /// Convenience method to add a test multiple times (to reproduce flaky tests)
  public addTestN(name: string, repeated: number, f: () => Promise<void>): void {
    for (let i = 0; i < repeated; i++) {
      this.addTest(`${name} ${i}`, f);
    }
  }

  public async run(): Promise<void> {
    let failed = 0;
    for (const test of this.tests) {
      try {
        await test.f();
        ok(`  ✔ ${test.name}`);
      } catch (e: any) {
        error(`  ✖︎ ${test.name}\n  ${e?.message || e}  ${e?.stack}`);
        failed += 1;
      }
    }
    if (failed) {
      const plural = failed != 1 ? "s" : "";
      throw new Error(`${failed} failed test${plural}`);
    }
  }
}

type CompResponse = vscode.CompletionList | vscode.CompletionItem[];

export class Context {
  expect!: typeof import("chai").expect;

  public async suite(name: string, f: (ctx: Suite) => void): Promise<void> {
    const ctx = new Suite();
    f(ctx);
    try {
      ok(`⌛︎ ${name}`);
      await ctx.run();
      ok(`✔ ${name}`);
    } catch (e: any) {
      error(`  ✖︎ ${name}\n  ${e?.message || e}  ${e?.stack}`);
      throw e;
    }
  }

  getWorkspace(workspace: string): vscode.Uri {
    return vscode.Uri.file(path.resolve(__dirname, "../../../e2e-workspaces/", workspace));
  }

  public async openWorkspace(workspace: string): Promise<void> {
    console.log(`Opening workspace ${workspace}`);

    const resolved = this.getWorkspace(workspace);
    // assert directory exists
    assert.ok(fs.existsSync(resolved.fsPath), "Workspace directory does not exist");

    vscode.workspace.updateWorkspaceFolders(0, vscode.workspace.workspaceFolders?.length || 0, {
      uri: resolved,
    });
    assert.ok(
      vscode.workspace.workspaceFolders?.length === 1 &&
        vscode.workspace.workspaceFolders[0].uri.toString() == resolved.toString(),
      // eslint-disable-next-line @typescript-eslint/no-base-to-string
      `Expected workspace folder to be ${resolved.toString()}, got ${vscode.workspace.workspaceFolders}`,
    );
  }

  workspaceUri(): vscode.Uri {
    const folders = vscode.workspace.workspaceFolders;
    assert.ok(folders, "No workspace folders");
    assert.strictEqual(folders.length, 1, "Expected exactly one workspace folder");
    return folders[0].uri;
  }

  async openDocument(docUri: vscode.Uri): Promise<vscode.TextEditor> {
    const doc = await vscode.workspace.openTextDocument(docUri);
    return await vscode.window.showTextDocument(doc);
  }

  async completion<T extends CompResponse = CompResponse>(
    docUri: vscode.Uri,
    position: vscode.Position,
  ): Promise<T> {
    const doc = await vscode.workspace.openTextDocument(docUri);
    const editor = await vscode.window.showTextDocument(doc);
    editor.selection = new vscode.Selection(position, position);
    return await vscode.commands.executeCommand(
      "vscode.executeCompletionItemProvider",
      docUri,
      position,
    );
  }

  diagTick = 0;
  diagnostics(
    cnt: number,
    f: () => Promise<any> = Promise.resolve,
    timeout = 5000,
  ): Promise<[vscode.DiagnosticChangeEvent, [vscode.Uri, vscode.Diagnostic[]][]]> {
    const diagNow = performance.now();

    this.diagTick += 1;
    const tick = this.diagTick;

    const received: any[] = [];
    return new Promise((resolve, reject) => {
      const doReject = (reason: string) => (err?: Error) => {
        console.error(
          `diagnostics[${tick}] ${reason}, expect ${cnt}, got ${JSON.stringify(received, undefined, 1)}`,
          err,
        );
        diagnosticsHandler.dispose();
        clearTimeout(t);
        reject(err || new Error(`diagnostics[${tick}] ${reason}`));
      };
      const t = setTimeout(doReject("timeout"), timeout);
      const diagnosticsHandler = vscode.languages.onDidChangeDiagnostics((e) => {
        const d = vscode.languages.getDiagnostics();
        // flatten the array with setting uri
        const diagnostics = d
          .map((e) => {
            for (const diag of e[1]) {
              (diag as any).uri = e[0];
            }
            return e[1];
          })
          .flat();
        received.push(diagnostics);

        if (cnt === undefined || cnt === diagnostics.length) {
          console.log(`diagnostics[${tick}] took`, performance.now() - diagNow, "ms");
          diagnosticsHandler.dispose();
          clearTimeout(t);
          resolve([e, d]);
        }
      });
      f().catch(doReject("error"));
    });
  }

  timeout(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
}

export async function run(): Promise<void> {
  const context = new Context();
  context.expect = (await importEsmModule<any>("chai")).expect;

  // exit process after timeout
  context.timeout(30000).then(() => {
    console.error("Tests timed out");
    process.exit(81);
  });

  const testFiles = (await readdir(path.resolve(__dirname))).filter((name) =>
    name.endsWith(".test.js"),
  );

  const filter = process.env.VSCODE_TEST_FILTER;
  if (filter) {
    console.log(`Running tests with filter: ${filter}`);
  }
  for (const testFile of testFiles) {
    try {
      console.log(`Running tests in ${testFile}`);
      if (filter && !testFile.includes(filter)) {
        continue;
      }

      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const testModule = require(path.resolve(__dirname, testFile));
      await testModule.getTests(context);
    } catch (e) {
      error(`${e}`);
      throw e;
    }
  }
}

function ok(message: string): void {
  console.log(`\x1b[32m${message}\x1b[0m`);
}

function error(message: string): void {
  console.error(`\x1b[31m${message}\x1b[0m`);
}

// https://stackoverflow.com/questions/65265420/how-to-prevent-typescript-from-transpiling-dynamic-imports-into-require
export async function importEsmModule<T>(name: string): Promise<T> {
  const module = eval(`(async () => {return await import("${name}")})()`);
  return module as T;
}
