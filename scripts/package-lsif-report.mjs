#!/usr/bin/env node

import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

const DEFAULT_REGISTRY = "https://packages.typst.org";

function parseArgs(argv) {
  const args = {
    out: "target/typst-knowledge-report",
    packageCachePath: "target/typst/packages",
    registryUrl: DEFAULT_REGISTRY,
    jobs: Number(process.env.TINYMIST_PACKAGE_LSIF_JOBS || 2),
    limit: undefined,
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
      case "--tinymist":
        args.tinymist = next();
        break;
      case "--package-cache-path":
        args.packageCachePath = next();
        break;
      case "--registry-url":
        args.registryUrl = trimTrailingSlash(next());
        break;
      case "--index-url":
        args.indexUrl = next();
        break;
      case "--jobs":
        args.jobs = Number.parseInt(next(), 10);
        break;
      case "--limit":
        args.limit = Number.parseInt(next(), 10);
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
  args.registryUrl = trimTrailingSlash(args.registryUrl);
  args.indexUrl ||= `${args.registryUrl}/preview/index.json`;
  args.out = path.resolve(args.out);
  args.packageCachePath = path.resolve(args.packageCachePath);
  args.tinymist = resolveCommand(args.tinymist);
  args.jobs = Math.min(args.jobs, Math.max(1, os.availableParallelism?.() || args.jobs));

  return args;
}

function resolveCommand(value) {
  if (path.isAbsolute(value) || value.startsWith(".") || value.includes("/") || value.includes("\\")) {
    return path.resolve(value);
  }
  return value;
}

function printHelp() {
  console.log(`Usage: node scripts/package-lsif-report.mjs [options]

Downloads all Typst registry packages, runs tinymist LSIF for each package
version, and writes an HTML report.

Options:
  --tinymist <path>              Path to the tinymist binary
  --out <dir>                   Report output directory
  --package-cache-path <dir>    Typst package cache root to populate
  --registry-url <url>          Registry base URL (default: ${DEFAULT_REGISTRY})
  --index-url <url>             Package index URL
  --jobs <n>                    Parallel LSIF jobs (default: 2)
  --limit <n>                   Process only the first n packages, for local smoke tests
`);
}

function trimTrailingSlash(value) {
  return value.replace(/\/+$/, "");
}

function normalizeIndexEntry(raw) {
  const name = raw.name ?? raw.package?.name;
  const version = raw.version ?? raw.package?.version;
  const namespace = raw.namespace || raw.package?.namespace || "preview";

  if (!name || !version) {
    throw new Error(`Malformed package index entry: ${JSON.stringify(raw)}`);
  }

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

async function fetchJson(url) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Failed to fetch ${url}: ${response.status} ${response.statusText}`);
  }
  return response.json();
}

async function downloadFile(url, outputPath) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Failed to fetch ${url}: ${response.status} ${response.statusText}`);
  }

  await fs.mkdir(path.dirname(outputPath), { recursive: true });
  const data = Buffer.from(await response.arrayBuffer());
  await fs.writeFile(outputPath, data);
}

function packageDir(cacheRoot, pkg) {
  return path.join(cacheRoot, pkg.namespace, pkg.name, pkg.version);
}

function archiveName(pkg) {
  return `${safeFileName(pkg.namespace)}-${safeFileName(pkg.name)}-${safeFileName(pkg.version)}.tar.gz`;
}

function lsifName(pkg) {
  return `${safeFileName(pkg.namespace)}-${safeFileName(pkg.name)}-${safeFileName(pkg.version)}.lsif.jsonl`;
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

async function downloadPackage(args, pkg) {
  const targetDir = packageDir(args.packageCachePath, pkg);
  const manifestPath = path.join(targetDir, "typst.toml");
  if (await exists(manifestPath)) {
    return { pkg, skipped: true };
  }

  const url = `${args.registryUrl}/${pkg.namespace}/${pkg.name}-${pkg.version}.tar.gz`;
  const archivePath = path.join(args.out, "downloads", archiveName(pkg));
  await downloadFile(url, archivePath);
  await fs.rm(targetDir, { recursive: true, force: true });
  await fs.mkdir(targetDir, { recursive: true });
  await run("tar", ["-xzf", archivePath, "-C", targetDir], { cwd: args.out });
  return { pkg, skipped: false };
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

async function generateLsif(args, pkg) {
  const workspace = path.join(args.out, "workspace");
  const input = path.join(workspace, "main.typ");
  await fs.mkdir(workspace, { recursive: true });
  if (!(await exists(input))) {
    await fs.writeFile(input, "\n");
  }

  const lsifPath = path.join(args.out, "lsif", lsifName(pkg));
  const statsPath = path.join(args.out, "stats", `${safeFileName(pkg.namespace)}-${safeFileName(pkg.name)}-${safeFileName(pkg.version)}.json`);
  await fs.mkdir(path.dirname(lsifPath), { recursive: true });
  await fs.mkdir(path.dirname(statsPath), { recursive: true });
  await fs.rm(lsifPath, { force: true });
  await fs.rm(statsPath, { force: true });

  const measured = await runMeasured(args.tinymist, [
    "query",
    "lsif",
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
    lsifPath,
    "--stats-output",
    statsPath,
    input,
  ]);

  const details = await inspectLsif(lsifPath);
  const analysis = await inspectAnalysisStats(statsPath);
  return {
    ...pkg,
    status: "ok",
    lsifPath,
    href: `lsif/${lsifName(pkg)}`,
    size: details.size,
    hash: details.hash,
    queries: details.queries,
    totalMs: measured.elapsedMs,
    maxRssKiB: measured.maxRssKiB,
    exprMs: analysis.exprMs,
    typeMs: analysis.typeMs,
  };
}

async function inspectLsif(filePath) {
  const stat = await fs.stat(filePath);
  const hash = createHash("sha256");
  let queries = 0;

  const input = createReadStream(filePath);
  input.on("data", (chunk) => hash.update(chunk));
  const lines = createInterface({ input, crlfDelay: Infinity });

  for await (const rawLine of lines) {
    const line = rawLine.trim();
    if (!line) {
      continue;
    }
    const entry = JSON.parse(line);
    if (entry.type === "edge" && entry.label === "next") {
      queries += 1;
    }
  }

  return {
    size: stat.size,
    hash: hash.digest("hex"),
    queries,
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

async function writeReport(args, rows) {
  const overallHash = hashOfHashes(rows);
  const failed = rows.filter((row) => row.status !== "ok");
  const generatedAt = new Date().toISOString();
  const durationChart = renderDurationChart(rows);

  const htmlRows = rows.map((row) => {
    const statusClass = row.status === "ok" ? "ok" : "failed";
    const detail = row.status === "ok"
      ? `<a href="${attr(row.href)}" data-lsif="${attr(row.href)}" data-package="${attr(row.displayId)}">View LSIF</a>`
      : `<span class="error">${escapeHtml(row.error)}</span>`;
    return `<tr class="${statusClass}">
  <td data-sort="${attr(row.displayId)}"><code>${escapeHtml(row.displayId)}</code></td>
  <td class="num" data-sort="${row.size ?? -1}">${row.size === undefined ? "" : escapeHtml(formatBytes(row.size))}</td>
  <td class="hash">${row.hash ? `<code>${escapeHtml(row.hash)}</code>` : ""}</td>
  <td class="num" data-sort="${row.queries ?? -1}">${row.queries ?? ""}</td>
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
      <div class="metric"><span>Overall LSIF hash</span><code>${escapeHtml(overallHash ?? "unavailable because one or more packages failed")}</code></div>
      <div class="metric"><span>Packages</span><strong>${rows.length}</strong></div>
      <div class="metric"><span>Failures</span><strong>${failed.length}</strong></div>
      <div class="metric"><span>Generated at</span><code>${escapeHtml(generatedAt)}</code></div>
    </section>
    <p>The overall hash is SHA-256 over the newline-separated per-package LSIF hashes in package id order. Query count is the number of LSIF <code>next</code> edges.</p>
    ${durationChart}
    <div class="toolbar">
      <input id="filter" type="search" placeholder="Filter packages" autocomplete="off">
    </div>
    <table>
      <thead>
        <tr>
          <th aria-sort="ascending"><button type="button" data-sort-column="0" data-sort-type="string">Package ID</button></th>
          <th><button type="button" data-sort-column="1" data-sort-type="number">LSIF Size</button></th>
          <th>LSIF Hash</th>
          <th><button type="button" data-sort-column="3" data-sort-type="number">Queries</button></th>
          <th><button type="button" data-sort-column="4" data-sort-type="number">Total Time</button></th>
          <th><button type="button" data-sort-column="5" data-sort-type="number">Max RSS</button></th>
          <th><button type="button" data-sort-column="6" data-sort-type="number">Expr Time</button></th>
          <th><button type="button" data-sort-column="7" data-sort-type="number">Type Time</button></th>
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
        <a id="viewer-raw" href="">Open raw LSIF</a>
      </header>
      <pre id="viewer-content"></pre>
    </section>
  </main>
  <script>
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
    for (const link of document.querySelectorAll("[data-lsif]")) {
      link.addEventListener("click", async (event) => {
        if (location.protocol === "file:") {
          return;
        }
        event.preventDefault();
        viewer.hidden = false;
        viewerTitle.textContent = link.dataset.package;
        viewerRaw.href = link.href;
        viewerContent.textContent = "Loading " + link.dataset.lsif + " ...";
        viewer.scrollIntoView({ block: "start" });
        try {
          const response = await fetch(link.href);
          if (!response.ok) {
            throw new Error(response.status + " " + response.statusText);
          }
          viewerContent.textContent = await response.text();
        } catch (error) {
          viewerContent.textContent = "Could not load LSIF into this page: " + error + "\\nUse the raw LSIF link above.";
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
    rows: rows.map((row) => ({
      id: row.displayId,
      spec: row.spec,
      status: row.status,
      size: row.size,
      hash: row.hash,
      queries: row.queries,
      totalMs: row.totalMs,
      maxRssKiB: row.maxRssKiB,
      exprMs: row.exprMs,
      typeMs: row.typeMs,
      href: row.href,
      error: row.error,
    })),
  };

  const summaryMd = [
    "### Typst knowledge report",
    "",
    `- Packages: ${rows.length}`,
    `- Failures: ${failed.length}`,
    `- Overall LSIF hash: \`${overallHash ?? "unavailable"}\``,
    `- Report entry: \`${path.relative(process.cwd(), path.join(args.out, "index.html"))}\``,
    "",
  ].join("\n");

  await fs.writeFile(path.join(args.out, "index.html"), html);
  await fs.writeFile(path.join(args.out, "summary.json"), `${JSON.stringify(summaryJson, null, 2)}\n`);
  await fs.writeFile(path.join(args.out, "summary.md"), summaryMd);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  await fs.mkdir(args.out, { recursive: true });
  await fs.mkdir(args.packageCachePath, { recursive: true });

  console.log(`Fetching package index: ${args.indexUrl}`);
  const rawIndex = await fetchJson(args.indexUrl);
  const rawPackages = Array.isArray(rawIndex) ? rawIndex : rawIndex.packages ?? rawIndex.entries;
  if (!Array.isArray(rawPackages)) {
    throw new Error("Package index must be an array or an object with a packages/entries array");
  }

  let packages = rawPackages
    .map(normalizeIndexEntry)
    .sort((left, right) => left.displayId.localeCompare(right.displayId, "en"));

  if (args.limit !== undefined) {
    packages = packages.slice(0, args.limit);
  }
  console.log(`Found ${packages.length} package versions`);

  console.log(`Downloading packages into ${args.packageCachePath}`);
  for (const [index, pkg] of packages.entries()) {
    process.stdout.write(`[download ${index + 1}/${packages.length}] ${pkg.displayId}\n`);
    await downloadPackage(args, pkg);
  }

  console.log(`Generating LSIF with ${args.jobs} parallel job(s)`);
  const rows = await mapLimit(packages, args.jobs, async (pkg, index) => {
    process.stdout.write(`[lsif ${index + 1}/${packages.length}] ${pkg.displayId}\n`);
    try {
      return await generateLsif(args, pkg);
    } catch (error) {
      return {
        ...pkg,
        status: "failed",
        error: error instanceof Error ? error.message : String(error),
      };
    }
  });

  await writeReport(args, rows);

  const failed = rows.filter((row) => row.status !== "ok");
  if (failed.length > 0) {
    console.error(`${failed.length} package(s) failed to generate LSIF`);
    for (const row of failed) {
      console.error(`- ${row.displayId}: ${row.error}`);
    }
    process.exit(1);
  }

  console.log(`Report written to ${path.join(args.out, "index.html")}`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : error);
  process.exit(1);
});
