import { execFileSync } from "child_process";
import fs from "fs";

const args = process.argv.slice(2);

let outputJson = false;
let targetArg;

for (const arg of args) {
  if (arg === "--json") {
    outputJson = true;
    continue;
  }

  if (!targetArg) {
    targetArg = arg;
    continue;
  }

  usage(`Unexpected argument: ${arg}`);
}

if (!targetArg) {
  usage("Missing target version");
}

const targetVersion = normalizeVersion(targetArg);
const targetReleaseType = classifyReleaseType(targetVersion);
const releaseNotesVersion = stripReleaseCandidateSuffix(targetVersion);
const targetTag = `v${targetVersion}`;
const releaseNotesTag = `v${releaseNotesVersion}`;
const expectedBranch = `bump-version-${targetVersion}`;
const trackedManifestCommand =
  "git ls-files --cached --others --exclude-standard | rg '(^|/)(Cargo\\.toml|package\\.json)$'";
const trackedManifests = listTrackedManifests(trackedManifestCommand);

const rootCargoPath = "Cargo.toml";
const rootCargo = readText(rootCargoPath);
const currentVersion = readWorkspaceVersion(rootCargo);
const currentBranch = readCurrentBranch();
const stableTags = listStableTags();
const previousStableTag = findPreviousStableTag(releaseNotesVersion, stableTags);
const changelog = inspectChangelog("editors/vscode/CHANGELOG.md", releaseNotesVersion);

const manifestPatches = trackedManifests.flatMap((manifestPath) =>
  buildVersionPatch(manifestPath, currentVersion, targetVersion, {
    kind: "manifest",
  }),
);

const releaseSensitiveFiles = buildReleaseSensitiveFiles(currentVersion, targetVersion);
const directReleaseSensitivePatches = releaseSensitiveFiles
  .filter((item) => item.kind === "direct-update" && item.command)
  .map((item) => ({
    path: item.path,
    lineUpdates: item.lineUpdates,
    patch: item.patch,
    command: item.command,
  }));
const generatedDocumentFollowUps = buildGeneratedDocumentFollowUps(targetVersion);
const versionUpdates = [...manifestPatches, ...directReleaseSensitivePatches].flatMap(
  (item) => item.lineUpdates,
);
const filePatches = [...manifestPatches, ...directReleaseSensitivePatches];

const releaseNotes = inspectReleaseNotes({
  releaseType: targetReleaseType,
  releaseNotesTag,
  previousStableTag,
  changelog,
});

const commands = buildCommands({
  targetVersion,
  targetReleaseType,
  releaseNotesVersion,
  releaseNotes,
  manifestPatches,
  directReleaseSensitivePatches,
  generatedDocumentFollowUps,
  changelog,
});

const releaseEntryPoints = buildReleaseEntryPoints({
  targetReleaseType,
  targetVersion,
  releaseNotesVersion,
});

const readiness = buildReadiness({
  currentVersion,
  targetVersion,
  currentBranch,
  expectedBranch,
  changelog,
  releaseNotes,
  releaseSensitiveFiles,
  manifestPatches,
  directReleaseSensitivePatches,
  generatedDocumentFollowUps,
});

const result = {
  targetVersion,
  targetTag,
  targetReleaseType,
  releaseNotesVersion,
  releaseNotesTag,
  currentVersion,
  expectedBranch,
  branch: {
    current: currentBranch,
    expected: expectedBranch,
    ready: currentBranch === expectedBranch,
  },
  trackedManifestCommand,
  trackedManifests,
  versionUpdates,
  filePatches,
  releaseSensitiveFiles,
  generatedDocumentFollowUps,
  changelog,
  releaseNotes,
  releaseEntryPoints,
  readiness,
  unmetPrerequisites: readiness.blockers,
  commands,
};

if (outputJson) {
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
} else {
  printHuman(result);
}

function usage(message) {
  if (message) {
    console.error(message);
  }

  console.error("Usage: node scripts/release-preflight.mjs <target-version> [--json]");
  process.exit(1);
}

function normalizeVersion(version) {
  return version.startsWith("v") ? version.slice(1) : version;
}

function classifyReleaseType(version) {
  if (/-rc[1-9]\d*$/.test(version)) {
    return "release-candidate";
  }

  const match = version.match(/^\d+\.\d+\.(\d+)$/);
  if (!match) {
    usage(`Invalid target version: ${version}`);
  }

  return Number(match[1]) % 2 === 1 ? "nightly" : "stable";
}

function stripReleaseCandidateSuffix(version) {
  return version.replace(/-rc[1-9]\d*$/, "");
}

function listTrackedManifests(command) {
  const output = execFileSync("sh", ["-c", command], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });

  return output
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .filter((filePath) => fs.existsSync(filePath));
}

function readText(filePath) {
  return fs.readFileSync(filePath, "utf8");
}

function readWorkspaceVersion(cargoToml) {
  const sectionMatch = cargoToml.match(/^\[workspace\.package\]\n([\s\S]*?)(?=^\[[^\]]+\]|\Z)/m);
  if (!sectionMatch) {
    throw new Error("Missing [workspace.package] in Cargo.toml");
  }

  const versionMatch = sectionMatch[1].match(/^version\s*=\s*"([^"]+)"/m);
  if (!versionMatch) {
    throw new Error("Missing workspace.package.version in Cargo.toml");
  }

  return versionMatch[1];
}

function readCurrentBranch() {
  try {
    return execFileSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();
  } catch (error) {
    return "(unknown)";
  }
}

function listStableTags() {
  try {
    const output = execFileSync("git", ["tag", "--list", "v*", "--sort=-version:refname"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });

    return output
      .split("\n")
      .map((line) => line.trim())
      .filter((tag) => /^v\d+\.\d+\.\d+$/.test(tag));
  } catch (error) {
    return [];
  }
}

function findPreviousStableTag(releaseVersion, stableTagsValue) {
  for (const tag of stableTagsValue) {
    const version = tag.slice(1);
    if (compareReleaseVersions(version, releaseVersion) < 0) {
      return tag;
    }
  }

  return null;
}

function compareReleaseVersions(left, right) {
  const leftParts = left.split(".").map((item) => Number(item));
  const rightParts = right.split(".").map((item) => Number(item));
  const length = Math.max(leftParts.length, rightParts.length);

  for (let index = 0; index < length; index += 1) {
    const leftValue = leftParts[index] ?? 0;
    const rightValue = rightParts[index] ?? 0;

    if (leftValue !== rightValue) {
      return leftValue - rightValue;
    }
  }

  return 0;
}

function buildVersionPatch(filePath, currentVersionValue, targetVersionValue, options = {}) {
  const originalLines = readText(filePath).split("\n");
  const lineUpdates = [];
  const nextLines = originalLines.map((line, index) => {
    if (!shouldReplaceVersionLine(filePath, line, currentVersionValue, options)) {
      return line;
    }

    const nextLine = line.replaceAll(currentVersionValue, targetVersionValue);
    if (nextLine === line) {
      return line;
    }

    lineUpdates.push({
      path: filePath,
      line: index + 1,
      currentLine: line,
      nextLine,
    });
    return nextLine;
  });

  if (lineUpdates.length === 0) {
    return [];
  }

  const patch = buildUnifiedPatch(filePath, originalLines, nextLines);

  return [
    {
      path: filePath,
      lineUpdates,
      patch,
      command: buildApplyPatchCommand(patch),
    },
  ];
}

function shouldReplaceVersionLine(filePath, line, currentVersionValue, options) {
  if (options.kind === "simple") {
    return line.includes(currentVersionValue);
  }

  if (line.trimStart().startsWith("#")) {
    return false;
  }

  if (filePath.endsWith("Cargo.toml")) {
    return (
      line.includes(`version = "${currentVersionValue}"`) ||
      (line.includes(`version = "=${currentVersionValue}"`) && !line.includes("tinymist-assets"))
    );
  }

  if (filePath.endsWith("package.json")) {
    return line.includes(`"version": "${currentVersionValue}"`);
  }

  return false;
}

function buildApplyPatchCommand(patch) {
  return `git apply <<'PATCH'\n${patch}\nPATCH`;
}

function buildUnifiedPatch(filePath, originalLines, nextLines) {
  const changedIndexes = originalLines.flatMap((line, index) =>
    line === nextLines[index] ? [] : [index],
  );

  if (changedIndexes.length === 0) {
    return "";
  }

  const hunks = buildHunks(originalLines, nextLines, changedIndexes, 3);
  return [
    `diff --git a/${filePath} b/${filePath}`,
    `--- a/${filePath}`,
    `+++ b/${filePath}`,
    ...hunks,
  ].join("\n");
}

function buildHunks(originalLines, nextLines, changedIndexes, contextSize) {
  const ranges = [];

  for (const index of changedIndexes) {
    const start = Math.max(0, index - contextSize);
    const end = Math.min(originalLines.length - 1, index + contextSize);
    const previous = ranges.at(-1);

    if (!previous || start > previous.end + 1) {
      ranges.push({ start, end });
      continue;
    }

    previous.end = Math.max(previous.end, end);
  }

  return ranges.flatMap((range) => {
    const originalCount = range.end - range.start + 1;
    const nextCount = range.end - range.start + 1;
    const header = `@@ -${formatHunkRange(range.start + 1, originalCount)} +${formatHunkRange(range.start + 1, nextCount)} @@`;
    const body = [];

    for (let index = range.start; index <= range.end; index += 1) {
      if (originalLines[index] === nextLines[index]) {
        body.push(` ${originalLines[index]}`);
        continue;
      }

      body.push(`-${originalLines[index]}`);
      body.push(`+${nextLines[index]}`);
    }

    return [header, ...body];
  });
}

function formatHunkRange(start, count) {
  return count === 1 ? `${start}` : `${start},${count}`;
}

function buildReleaseSensitiveFiles(currentVersionValue, targetVersionValue) {
  const directUpdateFiles = [
    {
      path: "editors/neovim/bootstrap.sh",
      reason: "Neovim bootstrap image tags should match the release version under test.",
    },
    {
      path: "editors/neovim/samples/lazyvim-dev/Dockerfile",
      reason: "Neovim sample Docker images should reference the matching tinymist release image.",
    },
  ];

  const directUpdates = directUpdateFiles.map((item) => {
    const patch = buildVersionPatch(item.path, currentVersionValue, targetVersionValue, {
      kind: "simple",
    })[0];

    return {
      path: item.path,
      kind: "direct-update",
      reason: item.reason,
      needsUpdate: Boolean(patch),
      lineUpdates: patch?.lineUpdates ?? [],
      patch: patch?.patch ?? null,
      command: patch?.command ?? null,
    };
  });

  const typliteGeneratedVersion = readGeneratedReleaseVersion("crates/typlite/README.md");

  return [
    ...directUpdates,
    {
      path: "crates/typlite/README.md",
      kind: "generated-document",
      reason:
        "Generated typlite install instructions embed the release download tag and should be refreshed from docs.",
      generatedByCommand: "node scripts/link-docs.mjs",
      detectedVersion: typliteGeneratedVersion,
      expectedVersion: targetVersionValue,
      needsRefresh: typliteGeneratedVersion !== targetVersionValue,
    },
  ];
}

function readGeneratedReleaseVersion(filePath) {
  const content = readText(filePath);
  const match = content.match(/releases\/download\/v([^/]+)\/typlite-installer\.(?:sh|ps1)/);
  return match?.[1] ?? null;
}

function buildGeneratedDocumentFollowUps(targetVersionValue) {
  return [
    {
      command: "node scripts/link-docs.mjs",
      outputs: ["crates/typlite/README.md"],
      reason: `Refresh generated documentation after version-bearing files are updated to ${targetVersionValue}.`,
    },
  ];
}

function inspectChangelog(filePath, releaseVersion) {
  const content = readText(filePath);
  const lines = content.split("\n");
  const headingPattern = new RegExp(`^## v${escapeForRegExp(releaseVersion)}\\b`);
  const headingIndex = lines.findIndex((line) => headingPattern.test(line));

  if (headingIndex === -1) {
    return {
      path: filePath,
      entryVersion: releaseVersion,
      status: "missing",
      hasEntry: false,
      heading: null,
      entryLines: [],
      itemCount: 0,
      pullRequests: [],
    };
  }

  let endIndex = lines.length;
  for (let index = headingIndex + 1; index < lines.length; index += 1) {
    if (/^## v\d+\.\d+\.\d+\b/.test(lines[index])) {
      endIndex = index;
      break;
    }
  }

  const entryLines = lines.slice(headingIndex + 1, endIndex);
  const entryText = entryLines.join("\n");
  const pullRequests = extractPullRequests(entryText);
  const itemCount = entryLines.filter((line) => line.trim().startsWith("* ")).length;

  return {
    path: filePath,
    entryVersion: releaseVersion,
    status: itemCount > 0 ? "present" : "empty",
    hasEntry: true,
    heading: lines[headingIndex],
    itemCount,
    entryLines,
    pullRequests,
  };
}

function inspectReleaseNotes({
  releaseType,
  releaseNotesTag,
  previousStableTag,
  changelog: changelogInfo,
}) {
  if (releaseType === "nightly") {
    return {
      applicable: false,
      status: "not-applicable",
      reason: "Nightly releases do not use the stable/release-candidate GitHub notes handoff.",
      command: null,
      previousStableTag,
      candidateItems: [],
      changelogSummary: {
        status: "not-applicable",
        represented: [],
        omitted: [],
      },
    };
  }

  if (!previousStableTag) {
    return {
      applicable: true,
      status: "unavailable",
      reason: `Could not find a previous stable tag before ${releaseNotesTag}.`,
      command: null,
      previousStableTag,
      candidateItems: [],
      changelogSummary: {
        status: "unavailable",
        reason: "Missing previous stable tag.",
        represented: [],
        omitted: [],
      },
    };
  }

  const command = buildReleaseNotesCommand(releaseNotesTag, previousStableTag);
  if (!isCommandAvailable("gh")) {
    return {
      applicable: true,
      status: "unavailable",
      reason: "`gh` is not installed, so GitHub-generated notes could not be fetched locally.",
      command,
      previousStableTag,
      candidateItems: [],
      changelogSummary: {
        status: "unavailable",
        reason: "`gh` is not installed.",
        represented: [],
        omitted: [],
      },
    };
  }

  try {
    const body = execFileSync(
      "gh",
      [
        "api",
        "repos/Myriad-Dreamin/tinymist/releases/generate-notes",
        "-f",
        `tag_name=${releaseNotesTag}`,
        "-f",
        `previous_tag_name=${previousStableTag}`,
        "--jq",
        ".body",
      ],
      {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      },
    );

    const candidateItems = parseReleaseNotesItems(body);
    return {
      applicable: true,
      status: "available",
      command,
      previousStableTag,
      candidateItems,
      changelogSummary: summarizeChangelogCoverage(candidateItems, changelogInfo),
    };
  } catch (error) {
    return {
      applicable: true,
      status: "unavailable",
      reason: extractProcessFailure(error),
      command,
      previousStableTag,
      candidateItems: [],
      changelogSummary: {
        status: "unavailable",
        reason: extractProcessFailure(error),
        represented: [],
        omitted: [],
      },
    };
  }
}

function buildReleaseNotesCommand(releaseNotesTag, previousStableTag) {
  return `gh api 'repos/Myriad-Dreamin/tinymist/releases/generate-notes' -f tag_name=${releaseNotesTag} -f previous_tag_name=${previousStableTag} --jq .body`;
}

function isCommandAvailable(command) {
  try {
    execFileSync("sh", ["-c", `command -v ${command} >/dev/null 2>&1`], {
      stdio: ["ignore", "ignore", "ignore"],
    });
    return true;
  } catch (error) {
    return false;
  }
}

function extractProcessFailure(error) {
  const stderr = error.stderr?.toString().trim();
  if (stderr) {
    const lines = stderr
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
    const preferred =
      lines.find((line) => /^error\b/i.test(line)) ||
      lines.find((line) => !line.startsWith("* ")) ||
      lines[0];
    if (preferred) {
      return preferred;
    }
  }

  const stdout = error.stdout?.toString().trim();
  if (stdout) {
    const lines = stdout
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
    const preferred =
      lines.find((line) => /^error\b/i.test(line)) ||
      lines.find((line) => !line.startsWith("* ")) ||
      lines[0];
    if (preferred) {
      return preferred;
    }
  }

  return error.message;
}

function parseReleaseNotesItems(body) {
  return body
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.startsWith("* ") && line.includes("pull/"))
    .map((line) => {
      const text = line.replace(/^\* /, "");
      return {
        text,
        pullRequests: extractPullRequests(text),
      };
    });
}

function summarizeChangelogCoverage(candidateItems, changelogInfo) {
  if (!changelogInfo.hasEntry) {
    return {
      status: "blocked",
      reason: `Missing changelog entry for v${changelogInfo.entryVersion}.`,
      represented: [],
      omitted: candidateItems,
    };
  }

  const represented = [];
  const omitted = [];

  for (const item of candidateItems) {
    const matchedPullRequests = item.pullRequests.filter((pullRequest) =>
      changelogInfo.pullRequests.includes(pullRequest),
    );

    if (matchedPullRequests.length > 0) {
      represented.push({
        ...item,
        matchedPullRequests,
      });
      continue;
    }

    omitted.push(item);
  }

  return {
    status: "available",
    represented,
    omitted,
  };
}

function extractPullRequests(text) {
  return [...text.matchAll(/pull\/(\d+)/g)].map((match) => Number(match[1]));
}

function buildCommands({
  targetVersion: targetVersionValue,
  targetReleaseType: releaseType,
  releaseNotesVersion: releaseNotesVersionValue,
  releaseNotes: releaseNotesInfo,
  manifestPatches: manifestPatchesValue,
  directReleaseSensitivePatches: directReleaseSensitivePatchesValue,
  generatedDocumentFollowUps: generatedFollowUps,
  changelog: changelogInfo,
}) {
  const updateCommands = [
    ...manifestPatchesValue.map((item) => item.command),
    ...directReleaseSensitivePatchesValue.map((item) => item.command),
  ];

  const reviewPaths = dedupe([
    ...manifestPatchesValue.map((item) => item.path),
    ...directReleaseSensitivePatchesValue.map((item) => item.path),
    changelogInfo.path,
    ...generatedFollowUps.flatMap((item) => item.outputs),
  ]);

  const versionSearchPaths = dedupe([
    ...manifestPatchesValue.map((item) => item.path),
    ...directReleaseSensitivePatchesValue.map((item) => item.path),
    ...generatedFollowUps.flatMap((item) => item.outputs),
  ]);

  const reviewCommands = [];
  if (reviewPaths.length > 0) {
    reviewCommands.push(`git diff -- ${reviewPaths.map(shellQuote).join(" ")}`);
  }

  if (versionSearchPaths.length > 0) {
    reviewCommands.push(
      `rg -n ${shellQuote(escapeForRipgrep(targetVersionValue))} ${versionSearchPaths.map(shellQuote).join(" ")}`,
    );
  }

  reviewCommands.push(
    `rg -n ${shellQuote(`^## v${escapeForRipgrep(releaseNotesVersionValue)}\\b`)} ${shellQuote(changelogInfo.path)}`,
  );

  const stagePaths = dedupe([
    ...manifestPatchesValue.map((item) => item.path),
    ...directReleaseSensitivePatchesValue.map((item) => item.path),
    changelogInfo.path,
    ...generatedFollowUps.flatMap((item) => item.outputs),
  ]);

  const prepareCommands = [
    ...generatedFollowUps.map((item) => item.command),
    stagePaths.length > 0 ? `git add .` : null,
    `git commit -m ${shellQuote(`build: bump version to ${targetVersionValue}`)}`,
  ].filter(Boolean);

  const checkCommands = [
    `node scripts/release-preflight.mjs ${shellQuote(targetVersionValue)} --json`,
  ];

  const handoffCommands = buildHandoffCommands({
    targetVersion: targetVersionValue,
    releaseType,
    releaseNotesVersion: releaseNotesVersionValue,
    releaseNotes: releaseNotesInfo,
  });

  return {
    update: updateCommands,
    review: reviewCommands,
    prepare: prepareCommands,
    check: checkCommands,
    handoff: handoffCommands,
  };
}

function buildHandoffCommands({
  targetVersion: targetVersionValue,
  releaseType,
  releaseNotesVersion: releaseNotesVersionValue,
  releaseNotes: releaseNotesInfo,
}) {
  if (releaseType === "nightly") {
    return [
      "gh workflow run release-nightly.yml -f release_type=nightly",
      "gh workflow run release-nightly.yml -f release_type=canary",
    ];
  }

  return [releaseNotesInfo.command, `yarn release ${targetVersionValue}`].filter(Boolean);
}

function buildReleaseEntryPoints({
  targetReleaseType: releaseType,
  targetVersion: targetVersionValue,
  releaseNotesVersion: releaseNotesVersionValue,
}) {
  if (releaseType === "nightly") {
    return [
      {
        path: "scripts/nightly-utils.mjs",
        command: "gh workflow run release-nightly.yml -f release_type=nightly",
        description: "Nightly release helper logic and workflow dispatch.",
        requiresApproval: true,
      },
      {
        path: ".github/workflows/release-nightly.yml",
        description: "Nightly/canary workflow entry point.",
      },
    ];
  }

  return [
    {
      path: "scripts/release.mjs",
      command: `yarn release ${targetVersionValue}`,
      description: "Create the release PR and dispatch the assets publish workflow.",
      requiresApproval: true,
    },
  ];
}

function buildReadiness({
  currentVersion: currentVersionValue,
  targetVersion: targetVersionValue,
  currentBranch: currentBranchValue,
  expectedBranch: expectedBranchValue,
  changelog: changelogInfo,
  releaseNotes: releaseNotesInfo,
  releaseSensitiveFiles: releaseSensitiveFilesValue,
  manifestPatches: manifestPatchesValue,
  directReleaseSensitivePatches: directReleaseSensitivePatchesValue,
  generatedDocumentFollowUps: generatedFollowUps,
}) {
  const blockers = [];
  const warnings = [];
  const pendingLocalPreparation = [];
  const currentVersionMatchesTarget = currentVersionValue === targetVersionValue;
  const generatedDocumentFiles = releaseSensitiveFilesValue.filter(
    (item) => item.kind === "generated-document" && item.needsRefresh,
  );

  if (currentBranchValue !== expectedBranchValue) {
    blockers.push({
      code: "branch-mismatch",
      message: `Current branch is ${currentBranchValue}; expected ${expectedBranchValue}.`,
    });
  }

  if (!changelogInfo.hasEntry) {
    blockers.push({
      code: "missing-changelog-entry",
      message: `Missing changelog entry for v${changelogInfo.entryVersion} in ${changelogInfo.path}.`,
    });
  } else if (changelogInfo.itemCount === 0) {
    blockers.push({
      code: "empty-changelog-entry",
      message: `Changelog entry ${changelogInfo.heading} does not contain any bullet items yet.`,
    });
  }

  if (manifestPatchesValue.length > 0) {
    pendingLocalPreparation.push({
      code: "manifest-version-updates",
      message: `Version-bearing manifests still reference ${currentVersionValue}.`,
      files: manifestPatchesValue.map((item) => item.path),
    });
  }

  if (directReleaseSensitivePatchesValue.length > 0) {
    pendingLocalPreparation.push({
      code: "release-sensitive-direct-updates",
      message: "Release-sensitive non-manifest files still need direct version updates.",
      files: directReleaseSensitivePatchesValue.map((item) => item.path),
    });
  }

  if (generatedDocumentFiles.length > 0) {
    pendingLocalPreparation.push({
      code: "generated-doc-refresh",
      message: `Generated docs should be refreshed with ${generatedFollowUps.map((item) => item.command).join(", ")}.`,
      files: generatedDocumentFiles.map((item) => item.path),
    });

    if (currentVersionMatchesTarget) {
      blockers.push({
        code: "stale-generated-docs",
        message: `Generated release documents are stale for ${targetVersionValue}; run ${generatedFollowUps
          .map((item) => item.command)
          .join(", ")}.`,
      });
    }
  }

  if (releaseNotesInfo.applicable && releaseNotesInfo.status === "unavailable") {
    warnings.push({
      code: "release-notes-unavailable",
      message: releaseNotesInfo.reason,
      command: releaseNotesInfo.command,
    });
  }

  if (
    releaseNotesInfo.changelogSummary?.status === "available" &&
    releaseNotesInfo.changelogSummary.omitted.length > 0
  ) {
    warnings.push({
      code: "changelog-omissions",
      message: `${releaseNotesInfo.changelogSummary.omitted.length} candidate release-note items are not represented in the changelog yet.`,
    });
  }

  const status =
    blockers.length > 0
      ? "blocked"
      : pendingLocalPreparation.length > 0
        ? "local-preparation-needed"
        : "ready";

  return {
    status,
    ready: status === "ready",
    blockers,
    warnings,
    pendingLocalPreparation,
  };
}

function printHuman(resultValue) {
  console.log(`Target: ${resultValue.targetTag}`);
  console.log(`Release type: ${resultValue.targetReleaseType}`);
  console.log(`Current version: ${resultValue.currentVersion}`);
  console.log(`Expected branch: ${resultValue.expectedBranch}`);
  console.log(
    `Current branch: ${resultValue.branch.current} (${resultValue.branch.ready ? "ready" : "not ready"})`,
  );
  console.log(`Readiness: ${resultValue.readiness.status}`);

  if (resultValue.readiness.blockers.length > 0) {
    console.log("");
    console.log("Blockers:");
    for (const blocker of resultValue.readiness.blockers) {
      console.log(`- ${blocker.message}`);
    }
  }

  if (resultValue.readiness.pendingLocalPreparation.length > 0) {
    console.log("");
    console.log("Pending local preparation:");
    for (const item of resultValue.readiness.pendingLocalPreparation) {
      console.log(`- ${item.message}`);
    }
  }

  if (resultValue.readiness.warnings.length > 0) {
    console.log("");
    console.log("Warnings:");
    for (const warning of resultValue.readiness.warnings) {
      console.log(`- ${warning.message}`);
    }
  }

  console.log("");
  console.log("Manifest scan command:");
  console.log(resultValue.trackedManifestCommand);

  console.log("");
  console.log("Patch commands:");
  printCommandList(resultValue.commands.update);

  console.log("");
  console.log("Generated-doc follow-ups:");
  for (const followUp of resultValue.generatedDocumentFollowUps) {
    console.log(`- ${followUp.command} (${followUp.outputs.join(", ")})`);
  }

  console.log("");
  console.log("Changelog:");
  if (!resultValue.changelog.hasEntry) {
    console.log(`- Missing entry for v${resultValue.changelog.entryVersion}`);
  } else {
    console.log(`- ${resultValue.changelog.heading}`);
    console.log(`- ${resultValue.changelog.itemCount} bullet item(s) detected`);
  }

  if (resultValue.releaseNotes.command) {
    console.log("");
    console.log("Release-notes handoff:");
    console.log(resultValue.releaseNotes.command);
    if (resultValue.releaseNotes.changelogSummary.status === "available") {
      console.log(
        `Represented items: ${resultValue.releaseNotes.changelogSummary.represented.length}`,
      );
      console.log(`Omitted items: ${resultValue.releaseNotes.changelogSummary.omitted.length}`);
    } else if (resultValue.releaseNotes.reason) {
      console.log(`Notes unavailable: ${resultValue.releaseNotes.reason}`);
    }
  }

  console.log("");
  console.log("Review commands:");
  printCommandList(resultValue.commands.review);

  console.log("");
  console.log("Prepare commands:");
  printCommandList(resultValue.commands.prepare);

  console.log("");
  console.log("Check commands:");
  printCommandList(resultValue.commands.check);

  console.log("");
  console.log("Handoff commands:");
  printCommandList(resultValue.commands.handoff);
}

function printCommandList(commands) {
  if (!commands || commands.length === 0) {
    console.log("(none)");
    return;
  }

  for (const command of commands) {
    console.log(command);
  }
}

function dedupe(values) {
  return [...new Set(values.filter(Boolean))];
}

function shellQuote(value) {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

function escapeForRipgrep(value) {
  return value.replace(/[\\.^$|?*+()[\]{}]/g, "\\$&");
}

function escapeForRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
