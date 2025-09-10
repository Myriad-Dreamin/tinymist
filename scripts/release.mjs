/// This script creates a release PR.
/// For documentation, see docs/release-instruction.md.

import { spawn as spawnSync } from "child_process";
import fs from "fs/promises";

let tag = process.argv[2];
if (tag.startsWith("v")) {
  tag = tag.slice(1);
}

console.log(`Create release PR v${tag}`);

const cwd = process.cwd();

/**
 * Spawns a command and return a promise that resolves to the code. The id is used to identify the command in the output and prepended to each line.
 * The line will be buffered and printed to avoid line crossing.
 *
 * @typedef {{
 *  code?: number,
 *  stdout: Buffer
 * }} GhResult
 *
 * @param {string} id
 * @param {string} cmd
 * @param {any} options
 * @returns {Promise<GhResult>}
 */
export function spawnAsync(id, cmd, options = {}) {
  return new Promise((resolve, reject) => {
    options = options ?? {};
    options.cwd = cwd;
    options.shell = true;
    options.stdio = "pipe";
    const child = spawnSync(cmd, options);

    const linePrinter = (stream, outStream, capture = false) => {
      return new Promise((resolve, reject) => {
        let out = "";
        /** @type {Uint8Array[]} */
        const capturedStdout = [];

        stream.on("data", (data) => {
          if (capture) {
            capturedStdout.push(data);
          }
          out += data;
          const lines = out.split("\n");
          while (lines.length > 1) {
            const line = lines.shift();
            outStream.write(`[${id}] ${line}\n`);
          }
          out = lines.join("\n");
        });
        stream.on("end", () => {
          if (out) {
            outStream.write(`[${id}] ${out}\n`);
          }
          resolve(Buffer.concat(capturedStdout));
        });
      });
    };

    const res = linePrinter(child.stdout, process.stdout, true);
    linePrinter(child.stderr, process.stderr, false);

    child.on("close", async (code) => {
      if (code !== 0) {
        reject(new Error(`Command ${cmd} failed with code ${code}`));
      }
      const stdout = await res;
      resolve({ code: code ?? undefined, stdout });
    });
  });
}

/**
 * Spawns a command and return a promise that resolves to the code. The id is used to identify the command in the output and prepended to each line.
 * The line will be buffered and printed to avoid line crossing.
 *
 * @param {string} id
 * @param {string} cmd
 * @param {any} options
 * @returns {Promise<Buffer>}
 */
function spawn(id, cmd, options = {}) {
  return spawnAsync(id, cmd, options).then((result) => {
    if (result.code !== 0) {
      throw new Error(`Command ${cmd} failed with code ${result.code}`);
    }
    return result.stdout;
  });
}

const releaseAssetId = "release-asset-crate.yml";
const currentBranch = (await spawn("current-branch", "git rev-parse --abbrev-ref HEAD"))
  .toString()
  .trim();

async function findWorkflowRunId(workflowId, branch) {
  const runs = JSON.parse(
    await spawn("workflow-run-list", `gh run list -w ${workflowId} --json headBranch,databaseId`),
  );

  console.log(runs, branch);
  const run = runs.find((run) => run.headBranch === branch);
  return run.databaseId;
}

async function tryFindWorkflowRunId(workflowId, branch) {
  for (let i = 0; i < 10; i++) {
    const runId = await findWorkflowRunId(workflowId, branch);
    if (runId) {
      return runId;
    }
    await new Promise((resolve) => setTimeout(resolve, 5000));
  }
  throw new Error(`Workflow run ${workflowId} for branch ${branch} not found`);
}

async function createReleaseAsset() {
  await spawn(
    `workflow-run`,
    `gh workflow run ${releaseAssetId} -r ${currentBranch.toString().trim()}`,
  );

  // get and wait last run id
  const runId = await tryFindWorkflowRunId(releaseAssetId, currentBranch);

  console.log(`Workflow run ${runId} started`);
  // watch and print runs
  await spawn(`workflow-run-watch`, `gh run watch ${runId}`);
}

await spawn(
  "pr-create",
  `gh pr create --title "build: bump version to ${tag}" --body "+tag v${tag}"`,
);
await createReleaseAsset();
const cargoToml = await fs.readFile("Cargo.toml", "utf-8");
const newCargoToml = cargoToml.replace(
  /tinymist-assets = { version = "[^"]+" }/,
  `tinymist-assets = { version = "=${tag}" }`,
);
await fs.writeFile("Cargo.toml", newCargoToml);
// sleep 10 seconds to wait for cargo.lock to be updated
await new Promise((resolve) => setTimeout(resolve, 20000));

// add, commit and push
await spawn("add", "git add Cargo.toml Cargo.lock");
await spawn("commit", `git commit -am 'build: bump assets to ${tag}'`);
await spawn("push", "git push");
