import * as vscode from "vscode";
import { FileLevelContext, IContext } from "../context";
import { VirtualConsole } from "../util";
import * as fs from "fs";

export function testingFeatureActivate(context: IContext) {
  const runTests =
    (kind: "cov" | "test") => (request: vscode.TestRunRequest, token: vscode.CancellationToken) =>
      runCoverageTests(kind, request, token);

  const testController = vscode.tests.createTestController(
    "tinymist-tests",
    "Typst Tests (Tinymist)",
  );

  const profileFileCoverage = testController.createRunProfile(
    "tinymist-profile-file-coverage",
    vscode.TestRunProfileKind.Coverage,
    runTests("cov"),
  );

  const profileTestCoverage = testController.createRunProfile(
    "tinymist-profile-test-coverage",
    vscode.TestRunProfileKind.Coverage,
    runTests("test"),
  );

  context.subscriptions.push(testController, profileFileCoverage, profileTestCoverage);

  const makeCommand = (kind: "cov" | "test") => async (ctx: FileLevelContext) => {
    if (!context.isValidEditor(ctx.currentEditor)) {
      return;
    }
    const document = ctx.currentEditor.document;

    const includes = [
      testController.createTestItem("tinymist-profile", "tinymist-profile", document.uri),
    ];

    const testRunRequest = new vscode.TestRunRequest(
      includes,
      undefined,
      kind == "cov" ? profileFileCoverage : profileTestCoverage,
      false,
      true,
    );

    const cc = new vscode.CancellationTokenSource();
    runCoverageTests(kind, testRunRequest, cc.token);
  };

  context.registerFileLevelCommand({
    command: "tinymist.profileCurrentFileCoverage",
    execute: makeCommand("cov"),
  });
  context.registerFileLevelCommand({
    command: "tinymist.profileCurrentTestCoverage",
    execute: makeCommand("test"),
  });

  async function runCoverageTests(
    testKind: "cov" | "test",
    request: vscode.TestRunRequest,
    token: vscode.CancellationToken,
  ) {
    const testRun = testController.createTestRun(request);
    if (request.include?.length !== 1) {
      context.showErrorMessage("Invalid tinymist test run request");
      return;
    }

    const item = request.include[0];
    const uri = item.uri;
    if (!uri) {
      context.showErrorMessage("Invalid tinymist test item uri");
      return;
    }
    testRun.started(item);
    const cwd = context.getRootForUri(uri);

    const failed = (msg: string) => {
      testRun.failed(item, new vscode.TestMessage(msg));
      testRun.end();
    };

    const kind = request.profile?.kind;

    const executable = context.tinymistExec;
    if (!executable) {
      failed("tinymist executable not found");
      testRun.end();
      return;
    }

    const vc = new VirtualConsole();
    const killer = new vscode.EventEmitter<void>();

    const terminal = vscode.window.createTerminal({
      name: "document Coverage",
      pty: {
        onDidWrite: vc.writeEmitter.event,
        open: () => {},
        close: () => {
          killer.fire();
        },
      },
    });
    terminal.show(true);

    const coverageTask = executable.execute(
      {
        cwd,
        killer,
        isTTY: true,
        stdout: (data: Buffer) => {
          vc.write(data.toString("utf8"));
        },
        stderr: (data: Buffer) => {
          vc.write(data.toString("utf8"));
        },
      },
      [testKind, uri.fsPath],
    );

    const detailsFut = coverageTask.then<Map<string, vscode.FileCoverageDetail[]>>((res) => {
      const details = new Map<string, vscode.FileCoverageDetail[]>();
      if (!res || res.code !== 0) {
        return details;
      }

      const covPath = vscode.Uri.joinPath(vscode.Uri.file(cwd!), "target/coverage.json").fsPath;
      console.log("covPath", covPath);
      if (!fs.existsSync(covPath)) {
        return details;
      }

      const cov = fs.readFileSync(covPath, "utf8");
      const cov_json: Record<string, vscode.StatementCoverage[]> = JSON.parse(cov);
      for (const [k, v] of Object.entries(cov_json)) {
        details.set(
          vscode.Uri.file(k).fsPath,
          v.map((x) => new vscode.StatementCoverage(x.executed, x.location, x.branches)),
        );
      }

      return details;
    });

    const p = request.profile;
    const requiredCoverage = p && kind === vscode.TestRunProfileKind.Coverage;
    if (requiredCoverage) {
      p.loadDetailedCoverage = async (_testRun, fi, token) => {
        await coverageTask;
        if (token.isCancellationRequested) {
          return [];
        }

        const details = await detailsFut;
        return details.get(fi.uri.fsPath) || [];
      };
    }

    const res = await coverageTask;

    if (!res) {
      return;
    }
    const { code } = res;
    vc.write(`\nCoverage profiling exited with code ${code}...`);
    if (code !== 0) {
      failed(`Coverage profiling exited with code ${code}`);
      return;
    }

    if (token.isCancellationRequested) {
      vc.write("\nCoverage profiling cancelled...");
      return;
    }

    if (requiredCoverage) {
      const details = await detailsFut;
      for (const [k, v] of details) {
        const uri = vscode.Uri.file(k);
        testRun.addCoverage(vscode.FileCoverage.fromDetails(uri, v));
      }
    }

    testRun.passed(item);
    testRun.end();
  }
}
