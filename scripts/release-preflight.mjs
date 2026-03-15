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
const trackedManifestCommand =
  "git ls-files --cached --others --exclude-standard | rg '(^|/)(Cargo\\.toml|package\\.json)$'";
const trackedManifests = listTrackedManifests(trackedManifestCommand);

const rootCargoPath = "Cargo.toml";
const rootCargo = readText(rootCargoPath);
const currentVersion = readWorkspaceVersion(rootCargo);
const filePatches = trackedManifests.flatMap((manifestPath) =>
  buildFilePatch(manifestPath, currentVersion, targetVersion),
);
const versionUpdates = filePatches.flatMap((item) => item.lineUpdates);
const updateCommands = filePatches.map((item) => item.command);
const matchedPaths = filePatches.map((item) => item.path);
const reviewCommands = matchedPaths.length
  ? [
      `git diff -- ${matchedPaths.map(shellQuote).join(" ")}`,
      `rg -n ${shellQuote(escapeForRipgrep(targetVersion))} ${matchedPaths.map(shellQuote).join(" ")}`,
    ]
  : [];
const handoffCommands = buildHandoffCommands(targetVersion, targetReleaseType, currentVersion);

const result = {
  targetVersion,
  targetTag: `v${targetVersion}`,
  targetReleaseType,
  currentVersion,
  trackedManifestCommand,
  trackedManifests,
  versionUpdates,
  filePatches,
  commands: {
    update: updateCommands,
    review: reviewCommands,
    handoff: handoffCommands,
  },
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

function buildFilePatch(filePath, currentVersionValue, targetVersionValue) {
  const originalLines = readText(filePath).split("\n");
  const lineUpdates = [];
  const nextLines = originalLines.map((line, index) => {
    if (line.trimStart().startsWith("#")) {
      return line;
    }

    if (filePath.endsWith("Cargo.toml")) {
      if (
        !line.includes(`version = "${currentVersionValue}"`) &&
        !(
          line.includes(`version = "=${currentVersionValue}"`) &&
          !line.includes("tinymist-assets")
        )
      ) {
        return line;
      }
    } else if (filePath.endsWith("package.json")) {
      if (!line.includes(`"version": "${currentVersionValue}"`)) {
        return line;
      }
    } else {
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

function buildHandoffCommands(targetVersionValue, releaseType, currentVersionValue) {
  if (releaseType === "nightly") {
    return [
      "gh workflow run release-nightly.yml -f release_type=nightly",
      "gh workflow run release-nightly.yml -f release_type=canary",
    ];
  }

  const notesTag = targetVersionValue.replace(/-rc[1-9]\d*$/, "");
  return [
    `gh api 'repos/Myriad-Dreamin/tinymist/releases/generate-notes' -f tag_name=v${notesTag} -f previous_tag_name=v${currentVersionValue} --jq .body`,
    `yarn release v${targetVersionValue}`,
    `git tag v${targetVersionValue}`,
    "git push --tag",
  ];
}

function printHuman(resultValue) {
  console.log(`Target: v${resultValue.targetVersion}`);
  console.log(`Release type: ${resultValue.targetReleaseType}`);
  console.log(`Current version: ${resultValue.currentVersion}`);
  console.log("");
  console.log("Manifest scan command:");
  console.log(resultValue.trackedManifestCommand);
  console.log("");
  console.log("Patch commands:");

  if (resultValue.commands.update.length === 0) {
    console.log("(none)");
  } else {
    for (const command of resultValue.commands.update) {
      console.log(command);
    }
  }

  console.log("");
  console.log("Review commands:");

  if (resultValue.commands.review.length === 0) {
    console.log("(none)");
  } else {
    for (const command of resultValue.commands.review) {
      console.log(command);
    }
  }

  console.log("");
  console.log("Handoff commands:");
  for (const command of resultValue.commands.handoff) {
    console.log(command);
  }
}

function shellQuote(value) {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

function escapeForRipgrep(value) {
  return value.replace(/[\\.^$|?*+()[\]{}]/g, "\\$&");
}
