import * as vscode from "vscode";
import { IContext } from "../../context";
import { VirtualConsole } from "../../util";
import * as fs from "fs";

export function testingCovFeatureActivate(
  context: IContext,
  testController: vscode.TestController,
) {
  const profileCoverage = testController.createRunProfile(
    "tinymist-profile-coverage",
    vscode.TestRunProfileKind.Coverage,
    runCoverageTests,
  );

  context.subscriptions.push(testController, profileCoverage);

  context.registerFileLevelCommand({
    command: "tinymist.profileCurrentFileCoverage",
    execute: async (ctx) => {
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
        profileCoverage,
        false,
        true,
      );

      const cc = new vscode.CancellationTokenSource();
      runCoverageTests(testRunRequest, cc.token);
    },
  });

  async function runCoverageTests(request: vscode.TestRunRequest, token: vscode.CancellationToken) {
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
        killer,
        isTTY: true,
        stdout: (data: Buffer) => {
          vc.write(data.toString("utf8"));
        },
        stderr: (data: Buffer) => {
          vc.write(data.toString("utf8"));
        },
      },
      ["cov", uri.fsPath],
    );

    const detailsFut = coverageTask.then<Map<string, vscode.FileCoverageDetail[]>>((res) => {
      const details = new Map<string, vscode.FileCoverageDetail[]>();
      if (!res || res.code !== 0) {
        return details;
      }

      const cov_path = "target/coverage.json";
      if (!fs.existsSync(cov_path)) {
        return details;
      }

      const cov = fs.readFileSync(cov_path, "utf8");
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
