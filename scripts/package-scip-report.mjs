#!/usr/bin/env node

import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { gzipSync } from "node:zlib";
import { pathToFileURL } from "node:url";

function parseArgs(argv) {
  const args = {
    out: "target/typst-knowledge-report",
    previewOut: undefined,
    packageCachePath: "typst-packages/packages",
    dataArtifactName: process.env.TINYMIST_KNOWLEDGE_DATA_ARTIFACT || "typst-knowledge-data",
    githubRepository: process.env.GITHUB_REPOSITORY || "",
    githubRunId: process.env.GITHUB_RUN_ID || "",
    jobs: Number(process.env.TINYMIST_PACKAGE_SCIP_JOBS || 2),
    limit: undefined,
    apiSnapshot: undefined,
    apiSnapshotInclude: parsePackageSpecArray(
      process.env.TINYMIST_API_SNAPSHOT_INCLUDE || "[]",
      "TINYMIST_API_SNAPSHOT_INCLUDE",
    ),
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = () => {
      const value = argv[i + 1];
      if (!value || value.startsWith("--")) {
        throw new Error(`Missing value for ${arg}`);
      }
      i += 1;
      return value;
    };

    switch (arg) {
      case "--help":
      case "-h":
        printHelp();
        process.exit(0);
      case "--out":
        args.out = next();
        break;
      case "--preview-out":
        args.previewOut = next();
        break;
      case "--tinymist":
        args.tinymist = next();
        break;
      case "--package-cache-path":
        args.packageCachePath = next();
        break;
      case "--data-artifact-name":
        args.dataArtifactName = next();
        break;
      case "--github-repository":
        args.githubRepository = next();
        break;
      case "--github-run-id":
        args.githubRunId = next();
        break;
      case "--jobs":
        args.jobs = Number.parseInt(next(), 10);
        break;
      case "--limit":
        args.limit = Number.parseInt(next(), 10);
        break;
      case "--api-snapshot":
        args.apiSnapshot = next();
        break;
      case "--api-snapshot-include":
        args.apiSnapshotInclude.push(
          ...parsePackageSpecArray(next(), "--api-snapshot-include"),
        );
        break;
      default:
        throw new Error(`Unknown argument: ${arg}`);
    }
  }

  if (!args.tinymist) {
    args.tinymist = process.platform === "win32"
      ? "target/release/tinymist.exe"
      : "target/release/tinymist";
  }
  if (!Number.isInteger(args.jobs) || args.jobs < 1) {
    throw new Error("--jobs must be a positive integer");
  }
  if (args.limit !== undefined && (!Number.isInteger(args.limit) || args.limit < 1)) {
    throw new Error("--limit must be a positive integer");
  }
  args.out = path.resolve(args.out);
  args.previewOut = path.resolve(args.previewOut || args.out);
  args.apiSnapshot = path.resolve(args.apiSnapshot || path.join(args.out, "api-snapshot.jsonl.gz"));
  args.packageCachePath = path.resolve(args.packageCachePath);
  args.tinymist = resolveCommand(args.tinymist);
  args.jobs = Math.min(args.jobs, Math.max(1, os.availableParallelism?.() || args.jobs));
  args.apiSnapshotInclude = Array.from(new Set(args.apiSnapshotInclude)).sort((left, right) => left.localeCompare(right, "en"));

  return args;
}

function parsePackageSpecArray(value, label) {
  let parsed;
  try {
    parsed = JSON.parse(value);
  } catch (error) {
    throw new Error(`${label} must be a JSON array of package specs: ${error.message}`);
  }
  if (!Array.isArray(parsed) || parsed.some((item) => typeof item !== "string")) {
    throw new Error(`${label} must be a JSON array of package specs`);
  }
  return parsed;
}

function resolveCommand(value) {
  if (path.isAbsolute(value) || value.startsWith(".") || value.includes("/") || value.includes("\\")) {
    return path.resolve(value);
  }
  return value;
}

function printHelp() {
  console.log(`Usage: node scripts/package-scip-report.mjs [options]

Scans a local typst/packages checkout, runs tinymist SCIP for each package
version, and writes an HTML report.

Options:
  --tinymist <path>              Path to the tinymist binary
  --out <dir>                   SCIP data output directory
  --preview-out <dir>           Single-file HTML preview output directory
  --package-cache-path <dir>    Local typst/packages "packages" directory
  --data-artifact-name <name>   GitHub Actions artifact name used by the HTML viewer
  --github-repository <repo>    GitHub repository in owner/name form
  --github-run-id <id>          GitHub Actions run id
  --jobs <n>                    Parallel SCIP jobs (default: 2)
  --limit <n>                   Process only the first n packages, for local smoke tests
  --api-snapshot <path>         Gzipped JSONL API snapshot path (default: <out>/api-snapshot.jsonl.gz)
  --api-snapshot-include <json> JSON array of package specs whose full packageDocs output is included
`);
}

function packageEntry(namespace, name, version) {
  const displayId = namespace === "preview"
    ? `${name}:${version}`
    : `${namespace}/${name}:${version}`;
  return {
    namespace,
    name: String(name),
    version: String(version),
    displayId,
    spec: `@${namespace}/${name}:${version}`,
  };
}

function packageDir(cacheRoot, pkg) {
  return path.join(cacheRoot, pkg.namespace, pkg.name, pkg.version);
}

function scipName(pkg) {
  return `${safeFileName(pkg.namespace)}-${safeFileName(pkg.name)}-${safeFileName(pkg.version)}.scip`;
}

function scipStatsName(pkg) {
  return `${safeFileName(pkg.namespace)}-${safeFileName(pkg.name)}-${safeFileName(pkg.version)}.json`;
}

function apiDocsName(pkg) {
  return `${safeFileName(pkg.namespace)}-${safeFileName(pkg.name)}-${safeFileName(pkg.version)}.md`;
}

function safeFileName(value) {
  return String(value).replace(/[^a-zA-Z0-9._-]/g, "_");
}

async function exists(filePath) {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

async function readDirectoryDirs(dir) {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  return entries
    .filter((entry) => entry.isDirectory() && !entry.name.startsWith("."))
    .map((entry) => entry.name)
    .sort((left, right) => left.localeCompare(right, "en"));
}

async function discoverPackages(packageCachePath) {
  if (!(await exists(packageCachePath))) {
    throw new Error(`Package directory does not exist: ${packageCachePath}`);
  }

  const packages = [];
  for (const namespace of await readDirectoryDirs(packageCachePath)) {
    const namespaceDir = path.join(packageCachePath, namespace);
    for (const name of await readDirectoryDirs(namespaceDir)) {
      const packageDirPath = path.join(namespaceDir, name);
      for (const version of await readDirectoryDirs(packageDirPath)) {
        const versionDir = path.join(packageDirPath, version);
        if (await exists(path.join(versionDir, "typst.toml"))) {
          packages.push(packageEntry(namespace, name, version));
        }
      }
    }
  }

  return packages.sort((left, right) => left.displayId.localeCompare(right.displayId, "en"));
}

async function run(command, commandArgs, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, commandArgs, {
      cwd: options.cwd,
      env: options.env || process.env,
      stdio: options.stdio || ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";

    child.stdout?.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr?.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", reject);
    child.on("close", (code) => {
      if (code === 0) {
        resolve({ stdout, stderr });
      } else {
        const rendered = [
          `${command} ${commandArgs.join(" ")} failed with exit code ${code}`,
          stdout.trim() && `stdout:\n${stdout.trim()}`,
          stderr.trim() && `stderr:\n${stderr.trim()}`,
        ].filter(Boolean).join("\n\n");
        reject(new Error(rendered));
      }
    });
  });
}

async function runMeasured(command, commandArgs, options = {}) {
  const canMeasureMemory = process.platform !== "win32" && await exists("/usr/bin/time");
  const marker = "TINYMIST_KNOWLEDGE_MAX_RSS_KIB=";
  const actualCommand = canMeasureMemory ? "/usr/bin/time" : command;
  const actualArgs = canMeasureMemory
    ? ["-f", `${marker}%M`, command, ...commandArgs]
    : commandArgs;
  const started = process.hrtime.bigint();

  try {
    const result = await run(actualCommand, actualArgs, options);
    const elapsedMs = Number(process.hrtime.bigint() - started) / 1_000_000;
    const parsed = parseMeasuredStderr(result.stderr, marker);
    return {
      ...result,
      stderr: parsed.stderr,
      elapsedMs,
      maxRssKiB: parsed.maxRssKiB,
    };
  } catch (error) {
    const elapsedMs = Number(process.hrtime.bigint() - started) / 1_000_000;
    if (error instanceof Error) {
      error.message += `\nmeasured elapsed: ${elapsedMs.toFixed(2)} ms`;
    }
    throw error;
  }
}

function parseMeasuredStderr(stderr, marker) {
  let maxRssKiB;
  const lines = stderr.split(/\r?\n/);
  const kept = [];
  for (const line of lines) {
    if (line.startsWith(marker)) {
      const value = Number.parseInt(line.slice(marker.length), 10);
      if (Number.isFinite(value)) {
        maxRssKiB = value;
      }
    } else {
      kept.push(line);
    }
  }
  return {
    stderr: kept.join("\n").trimEnd(),
    maxRssKiB,
  };
}

async function generateScip(args, pkg) {
  const workspace = path.join(args.out, "workspace");
  const input = path.join(workspace, "main.typ");
  await fs.mkdir(workspace, { recursive: true });
  if (!(await exists(input))) {
    await fs.writeFile(input, "\n");
  }

  const scipPath = path.join(args.out, "scip", scipName(pkg));
  const statsPath = path.join(args.out, "stats", scipStatsName(pkg));
  const scipStatsPath = path.join(args.out, "scip-stats", scipStatsName(pkg));
  await fs.mkdir(path.dirname(scipPath), { recursive: true });
  await fs.mkdir(path.dirname(statsPath), { recursive: true });
  await fs.mkdir(path.dirname(scipStatsPath), { recursive: true });
  await fs.rm(scipPath, { force: true });
  await fs.rm(statsPath, { force: true });
  await fs.rm(scipStatsPath, { force: true });

  const measured = await runMeasured(args.tinymist, [
    "query",
    "scip",
    "--root",
    workspace,
    "--package-path",
    args.packageCachePath,
    "--package-cache-path",
    args.packageCachePath,
    "--id",
    pkg.spec,
    "--path",
    packageDir(args.packageCachePath, pkg),
    "--output",
    scipPath,
    "--stats-output",
    statsPath,
    "--index-summary-output",
    scipStatsPath,
    input,
  ]);

  const details = await inspectScip(scipPath, scipStatsPath);
  const analysis = await inspectAnalysisStats(statsPath);
  const apiSnapshot = await generateApiSnapshotEntry(args, pkg, { workspace, input });
  return {
    ...pkg,
    status: "ok",
    scipPath,
    href: `scip-stats/${scipStatsName(pkg)}`,
    size: details.size,
    hash: details.hash,
    documents: details.documents,
    occurrences: details.occurrences,
    documentSymbols: details.documentSymbols,
    externalSymbols: details.externalSymbols,
    relationships: details.relationships,
    publicModules: details.publicModules,
    publicSymbols: details.publicSymbols,
    totalMs: measured.elapsedMs,
    maxRssKiB: measured.maxRssKiB,
    exprMs: analysis.exprMs,
    typeMs: analysis.typeMs,
    apiSnapshot,
    apiSize: apiSnapshot.bytes,
    apiHash: apiSnapshot.sha256,
    apiIncluded: apiSnapshot.included,
  };
}

async function generateApiSnapshotEntry(args, pkg, context) {
  const tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "tinymist-api-docs-"));
  const output = path.join(tempDir, apiDocsName(pkg));
  const includeContent = args.apiSnapshotInclude.includes(pkg.spec);

  try {
    await run(args.tinymist, [
      "query",
      "packageDocs",
      "--root",
      context.workspace,
      "--package-path",
      args.packageCachePath,
      "--package-cache-path",
      args.packageCachePath,
      "--id",
      pkg.spec,
      "--path",
      packageDir(args.packageCachePath, pkg),
      "--output",
      output,
      context.input,
    ]);

    const content = await normalizedPackageDocsOutput(output, args);
    const contentHash = hashBuffer(Buffer.from(content, "utf8"));
    const entry = {
      kind: "package",
      package: pkg.spec,
      displayId: pkg.displayId,
      bytes: contentHash.size,
      sha256: contentHash.hash,
      included: includeContent,
    };
    if (includeContent) {
      entry.content = content;
    }
    return entry;
  } finally {
    await fs.rm(tempDir, { recursive: true, force: true });
  }
}

async function normalizedPackageDocsOutput(filePath, args) {
  const content = await fs.readFile(filePath, "utf8");
  return normalizePackageDocsOutput(content, args);
}

function normalizePackageDocsOutput(content, args) {
  const replacements = packageDocsReplacements(args);
  const normalizedComments = content.replace(/<!--[\s\S]*?-->/g, (comment) => {
    return comment.replace(/(^|[^A-Za-z0-9+/=])([A-Za-z0-9+/]{16,}={0,2})(?=$|[^A-Za-z0-9+/=])/g, (match, prefix, token) => {
      let decoded;
      try {
        decoded = Buffer.from(token, "base64").toString("utf8");
      } catch {
        return match;
      }
      if (!decoded.startsWith("{") && !decoded.startsWith("[")) {
        return match;
      }

      let value;
      try {
        value = JSON.parse(decoded);
      } catch {
        return match;
      }
      const normalized = normalizeJsonStrings(value, replacements);
      return prefix + Buffer.from(JSON.stringify(normalized), "utf8").toString("base64");
    });
  });
  return replaceKnownPaths(normalizedComments, replacements);
}

function packageDocsReplacements(args) {
  const packageCachePath = path.resolve(args.packageCachePath);
  const slashPath = packageCachePath.split(path.sep).join("/");
  const fileUrl = pathToFileURL(packageCachePath).href;
  return uniqueReplacements([
    [fileUrl, "file:///$TYPST_PACKAGE_CACHE"],
    [packageCachePath, "$TYPST_PACKAGE_CACHE"],
    [slashPath, "$TYPST_PACKAGE_CACHE"],
  ]);
}

function uniqueReplacements(entries) {
  const seen = new Set();
  return entries
    .filter(([from]) => {
      if (!from || seen.has(from)) {
        return false;
      }
      seen.add(from);
      return true;
    })
    .sort(([left], [right]) => right.length - left.length)
    .map(([from, to]) => ({ from, to }));
}

function normalizeJsonStrings(value, replacements) {
  if (typeof value === "string") {
    return replaceKnownPaths(value, replacements);
  }
  if (Array.isArray(value)) {
    return value.map((item) => normalizeJsonStrings(item, replacements));
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [
        key,
        normalizeJsonStrings(item, replacements),
      ]),
    );
  }
  return value;
}

function replaceKnownPaths(value, replacements) {
  return replacements.reduce(
    (current, replacement) => current.split(replacement.from).join(replacement.to),
    value,
  );
}

function hashBuffer(buffer) {
  return {
    size: buffer.byteLength,
    hash: createHash("sha256").update(buffer).digest("hex"),
  };
}

async function hashFile(filePath) {
  const stat = await fs.stat(filePath);
  const hash = createHash("sha256");
  const input = createReadStream(filePath);
  for await (const chunk of input) {
    hash.update(chunk);
  }
  return {
    size: stat.size,
    hash: hash.digest("hex"),
  };
}

async function inspectScip(filePath, statsPath) {
  const fileHash = await hashFile(filePath);
  const stats = JSON.parse(await fs.readFile(statsPath, "utf8"));

  return {
    size: fileHash.size,
    hash: fileHash.hash,
    documents: stats.documents,
    occurrences: stats.occurrences,
    documentSymbols: stats.documentSymbols,
    externalSymbols: stats.externalSymbols,
    relationships: stats.relationships,
    publicModules: stats.publicModules,
    publicSymbols: stats.publicSymbols,
  };
}

async function inspectAnalysisStats(filePath) {
  const stats = JSON.parse(await fs.readFile(filePath, "utf8"));
  return {
    exprMs: queryTotalMs(stats, "expr_stage"),
    typeMs: queryTotalMs(stats, "type_check"),
  };
}

function queryTotalMs(stats, query) {
  const aggregate = stats.find((entry) => entry.file === null && entry.query === query);
  if (aggregate) {
    return aggregate.totalMs;
  }
  return stats
    .filter((entry) => entry.query === query)
    .reduce((sum, entry) => sum + (entry.totalMs || 0), 0);
}

async function mapLimit(items, limit, fn) {
  const results = new Array(items.length);
  let nextIndex = 0;

  async function worker() {
    while (nextIndex < items.length) {
      const index = nextIndex;
      nextIndex += 1;
      results[index] = await fn(items[index], index);
    }
  }

  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
  return results;
}

function hashOfHashes(rows) {
  if (rows.some((row) => row.status !== "ok")) {
    return null;
  }
  const input = rows.map((row) => row.hash).join("\n") + "\n";
  return createHash("sha256").update(input).digest("hex");
}

function hashOfApiHashes(rows) {
  if (rows.some((row) => row.status !== "ok" || !row.apiHash)) {
    return null;
  }
  const input = rows.map((row) => `${row.spec} ${row.apiHash}`).join("\n") + "\n";
  return createHash("sha256").update(input).digest("hex");
}

function sumDefined(rows, key) {
  return rows.reduce((sum, row) => sum + (Number.isFinite(row[key]) ? row[key] : 0), 0);
}

async function writeApiSnapshot(args, rows) {
  const metadata = {
    schema: 1,
    kind: "metadata",
    format: "tinymist query packageDocs markdown",
    contentEncoding: "utf8",
    hashAlgorithm: "sha256",
    normalization: ["packageCachePath"],
    fullContentPackages: args.apiSnapshotInclude,
  };
  const entries = [
    metadata,
    ...rows
      .filter((row) => row.apiSnapshot)
      .map((row) => row.apiSnapshot),
  ];
  const jsonl = entries.map((entry) => JSON.stringify(entry)).join("\n") + "\n";
  const compressed = gzipSync(Buffer.from(jsonl, "utf8"), { level: 9 });

  await fs.mkdir(path.dirname(args.apiSnapshot), { recursive: true });
  await fs.writeFile(args.apiSnapshot, compressed);

  return {
    path: args.apiSnapshot,
    relativePath: path.relative(args.out, args.apiSnapshot),
    hash: hashOfApiHashes(rows),
    packageCount: entries.length - 1,
    includedPackages: args.apiSnapshotInclude,
    bytes: compressed.byteLength,
  };
}

function formatBytes(bytes) {
  if (bytes === undefined) {
    return "";
  }
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${value} ${units[unit]}` : `${value.toFixed(2)} ${units[unit]}`;
}

function formatMs(ms) {
  if (ms === undefined) {
    return "";
  }
  if (ms >= 1000) {
    return `${(ms / 1000).toFixed(2)} s`;
  }
  return `${ms.toFixed(2)} ms`;
}

function formatMemory(kib) {
  if (kib === undefined) {
    return "";
  }
  const bytes = kib * 1024;
  return formatBytes(bytes);
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function attr(value) {
  return escapeHtml(value).replaceAll("`", "&#96;");
}

function scriptJson(value) {
  return JSON.stringify(value).replaceAll("</", "<\\/");
}

function performanceStats(rows) {
  const values = rows
    .filter((row) => row.status === "ok" && Number.isFinite(row.totalMs))
    .map((row) => ({ id: row.displayId, value: row.totalMs }))
    .sort((left, right) => right.value - left.value);

  if (values.length === 0) {
    return null;
  }

  const sum = values.reduce((acc, item) => acc + item.value, 0);
  const average = sum / values.length;
  const ascending = [...values].sort((left, right) => left.value - right.value);
  const mid = Math.floor(ascending.length / 2);
  const median = ascending.length % 2 === 0
    ? (ascending[mid - 1].value + ascending[mid].value) / 2
    : ascending[mid].value;

  return {
    values,
    average,
    median,
    max: values[0],
    min: values[values.length - 1],
  };
}

function renderDurationChart(rows) {
  const stats = performanceStats(rows);
  if (!stats) {
    return `<section class="chart-panel"><h2>Total analysis time</h2><p>No successful package measurements were collected.</p></section>`;
  }

  const width = 1100;
  const height = 320;
  const padLeft = 64;
  const padRight = 24;
  const padTop = 28;
  const padBottom = 70;
  const chartWidth = width - padLeft - padRight;
  const chartHeight = height - padTop - padBottom;
  const min = stats.min.value;
  const max = stats.max.value;
  const span = Math.max(1, max - min);
  const yFor = (value) => padTop + ((max - value) / span) * chartHeight;
  const xFor = (index) => padLeft + (stats.values.length === 1 ? 0 : (index / (stats.values.length - 1)) * chartWidth);
  const points = stats.values
    .map((item, index) => `${xFor(index).toFixed(2)},${yFor(item.value).toFixed(2)}`)
    .join(" ");
  const avgY = yFor(stats.average);
  const medianY = yFor(stats.median);
  const maxX = xFor(0);
  const maxY = yFor(stats.max.value);
  const minX = xFor(stats.values.length - 1);
  const minY = yFor(stats.min.value);

  return `<section class="chart-panel">
  <h2>Total analysis time</h2>
  <div class="chart-meta">
    <span>Average: <strong>${escapeHtml(formatMs(stats.average))}</strong></span>
    <span>Median: <strong>${escapeHtml(formatMs(stats.median))}</strong></span>
    <span>Max: <strong>${escapeHtml(formatMs(stats.max.value))}</strong> <code>${escapeHtml(stats.max.id)}</code></span>
    <span>Min: <strong>${escapeHtml(formatMs(stats.min.value))}</strong> <code>${escapeHtml(stats.min.id)}</code></span>
  </div>
  <svg viewBox="0 0 ${width} ${height}" role="img" aria-label="Package total analysis time sorted descending">
    <line class="axis" x1="${padLeft}" y1="${padTop + chartHeight}" x2="${width - padRight}" y2="${padTop + chartHeight}"></line>
    <line class="axis" x1="${padLeft}" y1="${padTop}" x2="${padLeft}" y2="${padTop + chartHeight}"></line>
    <line class="avg" x1="${padLeft}" y1="${avgY.toFixed(2)}" x2="${width - padRight}" y2="${avgY.toFixed(2)}"></line>
    <line class="median" x1="${padLeft}" y1="${medianY.toFixed(2)}" x2="${width - padRight}" y2="${medianY.toFixed(2)}"></line>
    <text class="guide-label" x="${width - padRight - 120}" y="${(avgY - 6).toFixed(2)}">avg ${escapeHtml(formatMs(stats.average))}</text>
    <text class="guide-label" x="${width - padRight - 120}" y="${(medianY + 16).toFixed(2)}">median ${escapeHtml(formatMs(stats.median))}</text>
    <polyline class="curve" points="${points}"></polyline>
    <circle class="point max-point" cx="${maxX.toFixed(2)}" cy="${maxY.toFixed(2)}" r="4"></circle>
    <circle class="point min-point" cx="${minX.toFixed(2)}" cy="${minY.toFixed(2)}" r="4"></circle>
    <text class="axis-label" x="${padLeft}" y="${padTop - 8}">${escapeHtml(formatMs(max))}</text>
    <text class="axis-label" x="${padLeft}" y="${padTop + chartHeight + 22}">${escapeHtml(formatMs(min))}</text>
    <text class="point-label" x="${Math.min(maxX + 8, width - padRight - 260).toFixed(2)}" y="${Math.max(maxY + 16, padTop + 16).toFixed(2)}">max ${escapeHtml(stats.max.id)}</text>
    <text class="point-label" x="${Math.max(padLeft, minX - 260).toFixed(2)}" y="${Math.min(minY - 10, padTop + chartHeight - 8).toFixed(2)}">min ${escapeHtml(stats.min.id)}</text>
  </svg>
</section>`;
}

async function writeReport(args, rows, apiSnapshotInfo) {
  const overallHash = hashOfHashes(rows);
  const failed = rows.filter((row) => row.status !== "ok");
  const successful = rows.filter((row) => row.status === "ok");
  const totalDocuments = sumDefined(successful, "documents");
  const totalOccurrences = sumDefined(successful, "occurrences");
  const totalDocumentSymbols = sumDefined(successful, "documentSymbols");
  const totalPublicSymbols = sumDefined(successful, "publicSymbols");
  const generatedAt = new Date().toISOString();
  const durationChart = renderDurationChart(rows);
  const artifactConfig = {
    repository: args.githubRepository,
    runId: args.githubRunId,
    name: args.dataArtifactName,
  };

  const htmlRows = rows.map((row) => {
    const statusClass = row.status === "ok" ? "ok" : "failed";
    const detail = row.status === "ok"
      ? `<a href="#viewer" data-scip-stats="${attr(row.href)}" data-package="${attr(row.displayId)}">View SCIP stats</a>`
      : `<span class="error">${escapeHtml(row.error)}</span>`;
    return `<tr class="${statusClass}">
  <td data-sort="${attr(row.displayId)}"><code>${escapeHtml(row.displayId)}</code></td>
  <td class="num" data-sort="${row.size ?? -1}">${row.size === undefined ? "" : escapeHtml(formatBytes(row.size))}</td>
  <td class="hash">${row.hash ? `<code>${escapeHtml(row.hash)}</code>` : ""}</td>
  <td class="num" data-sort="${row.documents ?? -1}">${row.documents ?? ""}</td>
  <td class="num" data-sort="${row.occurrences ?? -1}">${row.occurrences ?? ""}</td>
  <td class="num" data-sort="${row.documentSymbols ?? -1}">${row.documentSymbols ?? ""}</td>
  <td class="num" data-sort="${row.externalSymbols ?? -1}">${row.externalSymbols ?? ""}</td>
  <td class="num" data-sort="${row.publicSymbols ?? -1}">${row.publicSymbols ?? ""}</td>
  <td class="num" data-sort="${row.totalMs ?? -1}">${escapeHtml(formatMs(row.totalMs))}</td>
  <td class="num" data-sort="${row.maxRssKiB ?? -1}">${escapeHtml(formatMemory(row.maxRssKiB))}</td>
  <td class="num" data-sort="${row.exprMs ?? -1}">${escapeHtml(formatMs(row.exprMs))}</td>
  <td class="num" data-sort="${row.typeMs ?? -1}">${escapeHtml(formatMs(row.typeMs))}</td>
  <td>${detail}</td>
</tr>`;
  }).join("\n");

  const html = `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Tinymist Typst Knowledge Report</title>
  <style>
    :root {
      color-scheme: light dark;
      --bg: #f7f8fa;
      --fg: #1f2328;
      --muted: #57606a;
      --border: #d0d7de;
      --panel: #ffffff;
      --accent: #0969da;
      --danger: #cf222e;
      --success: #1a7f37;
    }
    @media (prefers-color-scheme: dark) {
      :root {
        --bg: #0d1117;
        --fg: #e6edf3;
        --muted: #8b949e;
        --border: #30363d;
        --panel: #161b22;
        --accent: #58a6ff;
        --danger: #ff7b72;
        --success: #3fb950;
      }
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background: var(--bg);
      color: var(--fg);
      font: 14px/1.45 system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    main {
      width: min(1600px, calc(100vw - 32px));
      margin: 24px auto 40px;
    }
    h1 {
      margin: 0 0 16px;
      font-size: 24px;
      line-height: 1.2;
    }
    .summary {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
      gap: 12px;
      margin-bottom: 18px;
    }
    .metric {
      border: 1px solid var(--border);
      background: var(--panel);
      border-radius: 8px;
      padding: 12px;
      min-width: 0;
    }
    .metric span {
      display: block;
      color: var(--muted);
      font-size: 12px;
      margin-bottom: 4px;
    }
    .metric code {
      overflow-wrap: anywhere;
    }
    .toolbar {
      display: flex;
      gap: 12px;
      align-items: center;
      margin: 18px 0 12px;
    }
    .toolbar input {
      width: min(420px, 100%);
      border: 1px solid var(--border);
      border-radius: 6px;
      background: var(--panel);
      color: var(--fg);
      padding: 8px 10px;
      font: inherit;
    }
    .chart-panel {
      margin: 18px 0;
      border: 1px solid var(--border);
      border-radius: 8px;
      background: var(--panel);
      padding: 14px;
    }
    .chart-panel h2 {
      margin: 0 0 10px;
      font-size: 16px;
    }
    .chart-meta {
      display: flex;
      flex-wrap: wrap;
      gap: 10px 18px;
      margin-bottom: 12px;
      color: var(--muted);
    }
    .chart-meta code {
      color: var(--fg);
    }
    .chart-panel svg {
      display: block;
      width: 100%;
      height: auto;
      overflow: visible;
    }
    .axis {
      stroke: var(--border);
      stroke-width: 1;
    }
    .curve {
      fill: none;
      stroke: var(--accent);
      stroke-width: 2.5;
      stroke-linejoin: round;
      stroke-linecap: round;
    }
    .avg {
      stroke: var(--success);
      stroke-width: 1.5;
      stroke-dasharray: 6 5;
    }
    .median {
      stroke: #bf8700;
      stroke-width: 1.5;
      stroke-dasharray: 3 4;
    }
    .point {
      fill: var(--panel);
      stroke-width: 2;
    }
    .max-point {
      stroke: var(--danger);
    }
    .min-point {
      stroke: var(--success);
    }
    .guide-label, .axis-label, .point-label {
      fill: var(--muted);
      font-size: 12px;
    }
    table {
      width: 100%;
      border-collapse: collapse;
      background: var(--panel);
      border: 1px solid var(--border);
    }
    th, td {
      border-bottom: 1px solid var(--border);
      padding: 8px 10px;
      text-align: left;
      vertical-align: top;
    }
    th {
      position: sticky;
      top: 0;
      background: var(--panel);
      z-index: 1;
      color: var(--muted);
      font-weight: 600;
    }
    th button {
      appearance: none;
      border: 0;
      padding: 0;
      background: transparent;
      color: inherit;
      font: inherit;
      font-weight: inherit;
      cursor: pointer;
    }
    th button::after {
      content: "sort";
      margin-left: 6px;
      color: var(--muted);
      font-size: 11px;
    }
    th[aria-sort="ascending"] button::after {
      content: "asc";
    }
    th[aria-sort="descending"] button::after {
      content: "desc";
    }
    tr:last-child td {
      border-bottom: 0;
    }
    .num {
      text-align: right;
      white-space: nowrap;
    }
    .hash code {
      overflow-wrap: anywhere;
    }
    a {
      color: var(--accent);
    }
    .failed td:first-child {
      border-left: 3px solid var(--danger);
    }
    .error {
      color: var(--danger);
      white-space: pre-wrap;
    }
    #viewer {
      margin-top: 20px;
      border: 1px solid var(--border);
      border-radius: 8px;
      background: var(--panel);
      overflow: hidden;
    }
    #viewer header {
      display: flex;
      justify-content: space-between;
      gap: 16px;
      align-items: center;
      padding: 10px 12px;
      border-bottom: 1px solid var(--border);
      color: var(--muted);
    }
    #viewer pre {
      margin: 0;
      max-height: 68vh;
      overflow: auto;
      padding: 12px;
      font-size: 12px;
      line-height: 1.4;
    }
  </style>
</head>
<body>
  <main>
    <h1>Tinymist Typst Knowledge Report</h1>
    <section class="summary">
      <div class="metric"><span>Overall SCIP hash</span><code>${escapeHtml(overallHash ?? "unavailable because one or more packages failed")}</code></div>
      <div class="metric"><span>Overall API hash</span><code>${escapeHtml(apiSnapshotInfo.hash ?? "unavailable because one or more packages failed")}</code></div>
      <div class="metric"><span>Packages</span><strong>${rows.length}</strong></div>
      <div class="metric"><span>Failures</span><strong>${failed.length}</strong></div>
      <div class="metric"><span>SCIP documents</span><strong>${totalDocuments}</strong></div>
      <div class="metric"><span>SCIP occurrences</span><strong>${totalOccurrences}</strong></div>
      <div class="metric"><span>SCIP document symbols</span><strong>${totalDocumentSymbols}</strong></div>
      <div class="metric"><span>Public symbols</span><strong>${totalPublicSymbols}</strong></div>
      <div class="metric"><span>API snapshot</span><code>${escapeHtml(apiSnapshotInfo.relativePath)}</code></div>
      <div class="metric"><span>Included API outputs</span><strong>${apiSnapshotInfo.includedPackages.length}</strong></div>
      <div class="metric"><span>Generated at</span><code>${escapeHtml(generatedAt)}</code></div>
    </section>
    <p>The overall SCIP hash is SHA-256 over the newline-separated per-package SCIP hashes in package id order. The overall API hash is SHA-256 over package spec and normalized <code>tinymist query packageDocs</code> output hashes; package-cache absolute paths and file URIs are replaced before hashing. Occurrence count is the number of SCIP symbol occurrences.</p>
    ${durationChart}
    <div class="toolbar">
      <input id="filter" type="search" placeholder="Filter packages" autocomplete="off">
    </div>
    <table>
      <thead>
        <tr>
          <th aria-sort="ascending"><button type="button" data-sort-column="0" data-sort-type="string">Package ID</button></th>
          <th><button type="button" data-sort-column="1" data-sort-type="number">SCIP Size</button></th>
          <th>SCIP Hash</th>
          <th><button type="button" data-sort-column="3" data-sort-type="number">Documents</button></th>
          <th><button type="button" data-sort-column="4" data-sort-type="number">Occurrences</button></th>
          <th><button type="button" data-sort-column="5" data-sort-type="number">Doc Symbols</button></th>
          <th><button type="button" data-sort-column="6" data-sort-type="number">External Symbols</button></th>
          <th><button type="button" data-sort-column="7" data-sort-type="number">Public Symbols</button></th>
          <th><button type="button" data-sort-column="8" data-sort-type="number">Total Time</button></th>
          <th><button type="button" data-sort-column="9" data-sort-type="number">Max RSS</button></th>
          <th><button type="button" data-sort-column="10" data-sort-type="number">Expr Time</button></th>
          <th><button type="button" data-sort-column="11" data-sort-type="number">Type Time</button></th>
          <th>Detail</th>
        </tr>
      </thead>
      <tbody id="rows">
${htmlRows}
      </tbody>
    </table>
    <section id="viewer" hidden>
      <header>
        <strong id="viewer-title"></strong>
        <a id="viewer-raw" href="">Open data artifact</a>
      </header>
      <pre id="viewer-content"></pre>
    </section>
  </main>
  <script>
    const artifactConfig = ${scriptJson(artifactConfig)};
    const filter = document.getElementById("filter");
    const tbody = document.getElementById("rows");
    let rows = Array.from(document.querySelectorAll("#rows tr"));
    filter.addEventListener("input", () => {
      const query = filter.value.trim().toLowerCase();
      for (const row of rows) {
        row.hidden = query && !row.cells[0].textContent.toLowerCase().includes(query);
      }
    });

    for (const button of document.querySelectorAll("[data-sort-column]")) {
      button.addEventListener("click", () => {
        const header = button.closest("th");
        const column = Number(button.dataset.sortColumn);
        const type = button.dataset.sortType;
        const current = header.getAttribute("aria-sort");
        const direction = current === "ascending" ? "descending" : "ascending";
        for (const th of header.parentElement.children) {
          th.removeAttribute("aria-sort");
        }
        header.setAttribute("aria-sort", direction);
        const multiplier = direction === "ascending" ? 1 : -1;
        rows = rows.sort((left, right) => {
          const leftValue = left.cells[column].dataset.sort ?? left.cells[column].textContent;
          const rightValue = right.cells[column].dataset.sort ?? right.cells[column].textContent;
          if (type === "number") {
            return (Number(leftValue) - Number(rightValue)) * multiplier;
          }
          return leftValue.localeCompare(rightValue, "en") * multiplier;
        });
        for (const row of rows) {
          tbody.appendChild(row);
        }
      });
    }

    const viewer = document.getElementById("viewer");
    const viewerTitle = document.getElementById("viewer-title");
    const viewerRaw = document.getElementById("viewer-raw");
    const viewerContent = document.getElementById("viewer-content");
    let artifactZipPromise;
    let artifactDownloadUrl = artifactConfig.repository && artifactConfig.runId
      ? "https://github.com/" + artifactConfig.repository + "/actions/runs/" + artifactConfig.runId + "#artifacts"
      : "";
    viewerRaw.href = artifactDownloadUrl || "#";

    async function findArtifactDownloadUrl() {
      if (!artifactConfig.repository || !artifactConfig.runId || !artifactConfig.name) {
        throw new Error("Missing GitHub Actions artifact metadata in this report.");
      }
      if (artifactDownloadUrl && artifactDownloadUrl.includes("/actions/artifacts/")) {
        return artifactDownloadUrl;
      }

      const listUrl = "https://api.github.com/repos/"
        + artifactConfig.repository
        + "/actions/runs/"
        + artifactConfig.runId
        + "/artifacts?per_page=100";
      const response = await fetch(listUrl, {
        headers: { Accept: "application/vnd.github+json" },
      });
      if (!response.ok) {
        throw new Error("artifact list request failed: " + response.status + " " + response.statusText);
      }

      const payload = await response.json();
      const artifacts = Array.isArray(payload.artifacts) ? payload.artifacts : [];
      const artifact = artifacts.find((item) => item.name === artifactConfig.name && !item.expired)
        || artifacts.find((item) => item.name === artifactConfig.name);
      if (!artifact) {
        throw new Error("artifact not found: " + artifactConfig.name);
      }

      artifactDownloadUrl = artifact.archive_download_url;
      viewerRaw.href = artifactDownloadUrl;
      return artifactDownloadUrl;
    }

    async function loadArtifactZip() {
      if (!artifactZipPromise) {
        artifactZipPromise = (async () => {
          const downloadUrl = await findArtifactDownloadUrl();
          const response = await fetch(downloadUrl, {
            headers: { Accept: "application/vnd.github+json" },
          });
          if (!response.ok) {
            throw new Error("artifact download failed: " + response.status + " " + response.statusText);
          }
          return response.arrayBuffer();
        })();
      }
      return artifactZipPromise;
    }

    function readU16(view, offset) {
      return view.getUint16(offset, true);
    }

    function readU32(view, offset) {
      return view.getUint32(offset, true);
    }

    function zipEntries(view) {
      const eocdSignature = 0x06054b50;
      const centralSignature = 0x02014b50;
      const minOffset = Math.max(0, view.byteLength - 0xffff - 22);
      let eocdOffset = -1;

      for (let offset = view.byteLength - 22; offset >= minOffset; offset -= 1) {
        if (readU32(view, offset) === eocdSignature) {
          eocdOffset = offset;
          break;
        }
      }
      if (eocdOffset < 0) {
        throw new Error("zip end-of-central-directory record was not found");
      }

      const totalEntries = readU16(view, eocdOffset + 10);
      let offset = readU32(view, eocdOffset + 16);
      const decoder = new TextDecoder();
      const entries = [];

      for (let index = 0; index < totalEntries; index += 1) {
        if (readU32(view, offset) !== centralSignature) {
          throw new Error("zip central directory is malformed");
        }

        const method = readU16(view, offset + 10);
        const compressedSize = readU32(view, offset + 20);
        const uncompressedSize = readU32(view, offset + 24);
        const nameLength = readU16(view, offset + 28);
        const extraLength = readU16(view, offset + 30);
        const commentLength = readU16(view, offset + 32);
        const localHeaderOffset = readU32(view, offset + 42);
        if (compressedSize === 0xffffffff || uncompressedSize === 0xffffffff || localHeaderOffset === 0xffffffff) {
          throw new Error("zip64 artifacts are not supported by this viewer");
        }

        const nameBytes = new Uint8Array(view.buffer, offset + 46, nameLength);
        entries.push({
          name: decoder.decode(nameBytes),
          method,
          compressedSize,
          uncompressedSize,
          localHeaderOffset,
        });
        offset += 46 + nameLength + extraLength + commentLength;
      }

      return entries;
    }

    async function inflateRaw(bytes) {
      if (!("DecompressionStream" in globalThis)) {
        throw new Error("this browser cannot decompress zip entries");
      }
      const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
      return new Uint8Array(await new Response(stream).arrayBuffer());
    }

    async function readZipText(buffer, fileName) {
      const view = new DataView(buffer);
      const entries = zipEntries(view);
      const entry = entries.find((item) => item.name === fileName)
        || entries.find((item) => item.name.endsWith("/" + fileName));
      if (!entry) {
        throw new Error("file not found in artifact: " + fileName);
      }

      const localSignature = 0x04034b50;
      const localOffset = entry.localHeaderOffset;
      if (readU32(view, localOffset) !== localSignature) {
        throw new Error("zip local file header is malformed");
      }

      const nameLength = readU16(view, localOffset + 26);
      const extraLength = readU16(view, localOffset + 28);
      const dataOffset = localOffset + 30 + nameLength + extraLength;
      const compressed = new Uint8Array(buffer, dataOffset, entry.compressedSize);
      let content;
      if (entry.method === 0) {
        content = compressed;
      } else if (entry.method === 8) {
        content = await inflateRaw(compressed);
      } else {
        throw new Error("unsupported zip compression method: " + entry.method);
      }

      if (entry.uncompressedSize !== 0 && content.byteLength !== entry.uncompressedSize) {
        throw new Error("zip entry size mismatch for " + fileName);
      }
      return new TextDecoder().decode(content);
    }

    for (const link of document.querySelectorAll("[data-scip-stats]")) {
      link.addEventListener("click", async (event) => {
        event.preventDefault();
        viewer.hidden = false;
        viewerTitle.textContent = link.dataset.package;
        viewerContent.textContent = "Loading " + link.dataset.scipStats + " ...";
        viewer.scrollIntoView({ block: "start" });
        try {
          const zip = await loadArtifactZip();
          const text = await readZipText(zip, link.dataset.scipStats);
          viewerContent.textContent = JSON.stringify(JSON.parse(text), null, 2);
        } catch (error) {
          viewerContent.textContent = "Could not load SCIP stats into this page: " + error + "\\nUse the data artifact link above.";
        }
      });
    }
  </script>
</body>
</html>
`;

  const summaryJson = {
    generatedAt,
    overallHash,
    packageCount: rows.length,
    failureCount: failed.length,
    scip: {
      documents: totalDocuments,
      occurrences: totalOccurrences,
      documentSymbols: totalDocumentSymbols,
      publicSymbols: totalPublicSymbols,
    },
    rows: rows.map((row) => ({
      id: row.displayId,
      spec: row.spec,
      status: row.status,
      size: row.size,
      hash: row.hash,
      documents: row.documents,
      occurrences: row.occurrences,
      documentSymbols: row.documentSymbols,
      externalSymbols: row.externalSymbols,
      relationships: row.relationships,
      publicModules: row.publicModules,
      publicSymbols: row.publicSymbols,
      totalMs: row.totalMs,
      maxRssKiB: row.maxRssKiB,
      exprMs: row.exprMs,
      typeMs: row.typeMs,
      apiSize: row.apiSize,
      apiHash: row.apiHash,
      apiIncluded: row.apiIncluded,
      href: row.href,
      error: row.error,
    })),
    apiSnapshot: {
      path: apiSnapshotInfo.relativePath,
      hash: apiSnapshotInfo.hash,
      packageCount: apiSnapshotInfo.packageCount,
      bytes: apiSnapshotInfo.bytes,
      includedPackages: apiSnapshotInfo.includedPackages,
    },
    artifact: artifactConfig,
  };

  const summaryMd = [
    "### Typst knowledge SCIP report",
    "",
    `- Packages: ${rows.length}`,
    `- Failures: ${failed.length}`,
    `- Overall SCIP hash: \`${overallHash ?? "unavailable"}\``,
    `- Overall API hash: \`${apiSnapshotInfo.hash ?? "unavailable"}\``,
    `- API snapshot: \`${apiSnapshotInfo.relativePath}\` (${formatBytes(apiSnapshotInfo.bytes)})`,
    `- Included API outputs: ${apiSnapshotInfo.includedPackages.length}`,
    `- SCIP documents: ${totalDocuments}`,
    `- SCIP occurrences: ${totalOccurrences}`,
    `- SCIP document symbols: ${totalDocumentSymbols}`,
    `- Public symbols: ${totalPublicSymbols}`,
    `- Preview entry: \`${path.relative(process.cwd(), path.join(args.previewOut, "index.html"))}\``,
    `- Data artifact: \`${args.dataArtifactName}\``,
    "",
  ].join("\n");

  await fs.mkdir(args.previewOut, { recursive: true });
  await fs.writeFile(path.join(args.previewOut, "index.html"), html);
  await fs.writeFile(path.join(args.out, "summary.json"), `${JSON.stringify(summaryJson, null, 2)}\n`);
  await fs.writeFile(path.join(args.out, "summary.md"), summaryMd);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  await fs.mkdir(args.out, { recursive: true });

  console.log(`Scanning packages in ${args.packageCachePath}`);
  let packages = await discoverPackages(args.packageCachePath);
  if (packages.length === 0) {
    throw new Error(`No Typst packages found in ${args.packageCachePath}`);
  }

  if (args.limit !== undefined) {
    packages = packages.slice(0, args.limit);
  }
  console.log(`Found ${packages.length} package versions`);

  console.log(`Generating SCIP with ${args.jobs} parallel job(s)`);
  const rows = await mapLimit(packages, args.jobs, async (pkg, index) => {
    process.stdout.write(`[scip ${index + 1}/${packages.length}] ${pkg.displayId}\n`);
    try {
      return await generateScip(args, pkg);
    } catch (error) {
      return {
        ...pkg,
        status: "failed",
        error: error instanceof Error ? error.message : String(error),
      };
    }
  });

  const apiSnapshotInfo = await writeApiSnapshot(args, rows);
  await writeReport(args, rows, apiSnapshotInfo);

  const failed = rows.filter((row) => row.status !== "ok");
  if (failed.length > 0) {
    console.error(`${failed.length} package(s) failed to generate SCIP`);
    for (const row of failed) {
      console.error(`- ${row.displayId}: ${row.error}`);
    }
    process.exit(1);
  }

  console.log(`Report written to ${path.join(args.previewOut, "index.html")}`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : error);
  process.exit(1);
});
