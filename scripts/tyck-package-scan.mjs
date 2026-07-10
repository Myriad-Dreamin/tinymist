#!/usr/bin/env node

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

const DEFAULT_TIMEOUT_MS = 10 * 60 * 1000;
const OUTPUT_LIMIT_BYTES = 1024 * 1024;
const DEFAULT_MAX_CELL_CHARS = 8192;
const activeChildren = new Set();

function parseArgs(argv) {
  const args = {
    out: "target/tyck-package-scan",
    packageCachePath:
      process.env.TINYMIST_PACKAGE_CACHE_PATH ||
      process.env.TYPST_PACKAGE_CACHE_PATH ||
      "typst-packages/packages",
    packagePath: process.env.TINYMIST_PACKAGE_PATH || process.env.TYPST_PACKAGE_PATH,
    jobs: Number(process.env.TINYMIST_TYCK_PACKAGE_SCAN_JOBS || 2),
    limit: undefined,
    commandTimeoutMs: Number(process.env.TINYMIST_PACKAGE_COMMAND_TIMEOUT_MS || DEFAULT_TIMEOUT_MS),
    namespace: undefined,
    packageName: undefined,
    version: undefined,
    latestOnly: false,
    skipExisting: false,
    allowFailures: false,
    keepJson: false,
    html: true,
    maxCellChars: Number(
      process.env.TINYMIST_TYCK_PACKAGE_SCAN_MAX_CELL_CHARS || DEFAULT_MAX_CELL_CHARS,
    ),
    githubBaseUrl:
      process.env.TINYMIST_TYCK_GITHUB_BASE_URL ||
      "https://github.com/typst/packages/blob/main/packages",
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
        break;
      case "--tinymist":
        args.tinymist = next();
        break;
      case "--out":
        args.out = next();
        break;
      case "--package-cache-path":
        args.packageCachePath = next();
        break;
      case "--package-path":
        args.packagePath = next();
        break;
      case "--jobs":
        args.jobs = Number.parseInt(next(), 10);
        break;
      case "--limit":
        args.limit = Number.parseInt(next(), 10);
        break;
      case "--command-timeout-ms":
        args.commandTimeoutMs = Number.parseInt(next(), 10);
        break;
      case "--namespace":
        args.namespace = next();
        break;
      case "--package":
        args.packageName = next();
        break;
      case "--version":
        args.version = next();
        break;
      case "--latest-only":
        args.latestOnly = true;
        break;
      case "--skip-existing":
        args.skipExisting = true;
        break;
      case "--allow-failures":
        args.allowFailures = true;
        break;
      case "--keep-json":
        args.keepJson = true;
        break;
      case "--no-html":
        args.html = false;
        break;
      case "--github-base-url":
        args.githubBaseUrl = next();
        break;
      case "--max-cell-chars":
        args.maxCellChars = Number.parseInt(next(), 10);
        break;
      default:
        throw new Error(`Unknown argument: ${arg}`);
    }
  }

  if (!args.tinymist) {
    args.tinymist = defaultTinymistBinary();
  }
  if (!Number.isInteger(args.jobs) || args.jobs < 1) {
    throw new Error("--jobs must be a positive integer");
  }
  if (args.limit !== undefined && (!Number.isInteger(args.limit) || args.limit < 1)) {
    throw new Error("--limit must be a positive integer");
  }
  if (!Number.isInteger(args.commandTimeoutMs) || args.commandTimeoutMs < 1) {
    throw new Error("--command-timeout-ms must be a positive integer");
  }
  if (args.skipExisting && !args.keepJson) {
    throw new Error("--skip-existing requires --keep-json");
  }
  if (!Number.isInteger(args.maxCellChars) || args.maxCellChars < 1) {
    throw new Error("--max-cell-chars must be a positive integer");
  }

  args.out = path.resolve(args.out);
  args.packageCachePath = path.resolve(args.packageCachePath);
  args.packagePath = path.resolve(args.packagePath || args.packageCachePath);
  args.tinymist = resolveCommand(args.tinymist);
  args.jobs = Math.min(args.jobs, Math.max(1, os.availableParallelism?.() || args.jobs));

  return args;
}

function defaultTinymistBinary() {
  const extension = process.platform === "win32" ? ".exe" : "";
  const release = `target/release/tinymist${extension}`;
  if (existsSync(release)) {
    return release;
  }
  return release;
}

function resolveCommand(value) {
  if (isPathLike(value)) {
    return path.resolve(value);
  }
  return value;
}

function isPathLike(value) {
  return (
    path.isAbsolute(value) || value.startsWith(".") || value.includes("/") || value.includes("\\")
  );
}

function printHelp() {
  console.log(`Usage: node scripts/tyck-package-scan.mjs [options]

Scans a local typst/packages checkout and writes Tinymist's package tyck scope
graph for each package version. By default the script writes a small, stable
set of diffable files: scope-graph.txt, type-mappings.tsv, packages.tsv, and
summary.json. Per-package JSON is temporary unless --keep-json is passed.

Options:
  --tinymist <path>              Path to the tinymist binary
  --out <dir>                   Output directory (default: target/tyck-package-scan)
  --package-cache-path <dir>    Local typst/packages "packages" directory
  --package-path <dir>          Package path passed to tinymist (default: package cache path)
  --jobs <n>                    Parallel package jobs (default: 2)
  --limit <n>                   Process only the first n package versions
  --command-timeout-ms <n>      Per tinymist command timeout (default: 600000)
  --namespace <name>            Restrict scan to one namespace
  --package <name>              Restrict scan to one package name
  --version <version>           Restrict scan to one package version
  --latest-only                 Scan only the latest version of each package
  --skip-existing               Reuse existing successful per-package JSON results (requires --keep-json)
  --allow-failures              Exit zero even when some packages fail
  --keep-json                   Keep per-package JSON dumps under <out>/packages
  --no-html                     Do not write per-package HTML reports
  --github-base-url <url>       GitHub base URL for source links
                                (default: https://github.com/typst/packages/blob/main/packages)
  --max-cell-chars <n>          Maximum rendered text per table/text cell
                                (default: ${DEFAULT_MAX_CELL_CHARS})
`);
}

function packageEntry(namespace, name, version) {
  const displayId =
    namespace === "preview" ? `${name}:${version}` : `${namespace}/${name}:${version}`;
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

function safeFileName(value) {
  return String(value).replace(/[^a-zA-Z0-9._-]/g, "_");
}

function packageFileStem(pkg) {
  return `${safeFileName(pkg.namespace)}-${safeFileName(pkg.name)}-${safeFileName(pkg.version)}`;
}

function resultName(pkg) {
  return `${packageFileStem(pkg)}.json`;
}

function htmlName(pkg) {
  return `${packageFileStem(pkg)}.html`;
}

function htmlRelativePath(pkg) {
  return `html/${htmlName(pkg)}`;
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

async function discoverPackages(args) {
  if (!(await exists(args.packageCachePath))) {
    throw new Error(`Package directory does not exist: ${args.packageCachePath}`);
  }

  const packages = [];
  for (const namespace of await readDirectoryDirs(args.packageCachePath)) {
    if (args.namespace && namespace !== args.namespace) {
      continue;
    }

    const namespaceDir = path.join(args.packageCachePath, namespace);
    for (const name of await readDirectoryDirs(namespaceDir)) {
      if (args.packageName && name !== args.packageName) {
        continue;
      }

      const packageDirPath = path.join(namespaceDir, name);
      for (const version of await readDirectoryDirs(packageDirPath)) {
        if (args.version && version !== args.version) {
          continue;
        }

        const versionDir = path.join(packageDirPath, version);
        if (await exists(path.join(versionDir, "typst.toml"))) {
          packages.push(packageEntry(namespace, name, version));
        }
      }
    }
  }

  const selected = args.latestOnly ? latestPackages(packages) : packages;
  return selected.sort((left, right) => left.displayId.localeCompare(right.displayId, "en"));
}

function latestPackages(packages) {
  const latest = new Map();
  for (const pkg of packages) {
    const key = `${pkg.namespace}/${pkg.name}`;
    const current = latest.get(key);
    if (!current || compareVersion(pkg.version, current.version) > 0) {
      latest.set(key, pkg);
    }
  }
  return Array.from(latest.values());
}

function compareVersion(left, right) {
  return String(left).localeCompare(String(right), "en", {
    numeric: true,
    sensitivity: "base",
  });
}

async function prepareWorkspace(args) {
  const workspace = path.join(args.out, ".tmp", "workspace");
  const input = path.join(workspace, "main.typ");
  await fs.mkdir(workspace, { recursive: true });
  if (!(await exists(input))) {
    await fs.writeFile(input, "\n");
  }
  return { workspace, input };
}

async function scanPackage(args, context, pkg, progress, index, writer) {
  const outputPath = args.keepJson
    ? path.join(args.out, "packages", resultName(pkg))
    : path.join(args.out, ".tmp", "packages", resultName(pkg));
  if (args.skipExisting && (await exists(outputPath))) {
    const existing = JSON.parse(await fs.readFile(outputPath, "utf8"));
    if (existing.status === "ok" && existing.scopeGraph) {
      logProgress(`[pkg ${progress}] ${pkg.displayId} skipped`);
      const rendered = await renderPackageOutput(args, pkg, existing);
      await writer.write(index, rendered);
      return summarizePackageResult(pkg, outputPath, existing, true, rendered);
    }
  }

  logProgress(`[pkg ${progress}] ${pkg.displayId} start`);
  const startedAt = new Date().toISOString();

  let row;
  try {
    await fs.mkdir(path.dirname(outputPath), { recursive: true });
    await fs.rm(outputPath, { force: true });

    const measured = await runMeasured(
      args.tinymist,
      [
        "query",
        "tyckScope",
        "--root",
        context.workspace,
        "--package-path",
        args.packagePath,
        "--package-cache-path",
        args.packageCachePath,
        "--id",
        pkg.spec,
        "--path",
        packageDir(args.packageCachePath, pkg),
        "--output",
        outputPath,
        "--max-type-chars",
        String(args.maxCellChars),
        context.input,
      ],
      {
        timeoutMs: args.commandTimeoutMs,
      },
    );

    const scopeGraph = JSON.parse(await fs.readFile(outputPath, "utf8"));
    const result = {
      schema: 1,
      ...pkg,
      status: "ok",
      startedAt,
      finishedAt: new Date().toISOString(),
      elapsedMs: measured.elapsedMs,
      maxRssKiB: measured.maxRssKiB,
      scopeGraph,
    };
    if (args.keepJson) {
      await writeJson(outputPath, result);
    }
    const rendered = await renderPackageOutput(args, pkg, result);
    await writer.write(index, rendered);
    if (!args.keepJson) {
      await fs.rm(outputPath, { force: true });
    }
    row = summarizePackageResult(pkg, outputPath, result, false, rendered);
    logProgress(`[pkg ${progress}] ${pkg.displayId} done ${formatMs(measured.elapsedMs)}`);
  } catch (error) {
    const result = {
      schema: 1,
      ...pkg,
      status: "failed",
      startedAt,
      finishedAt: new Date().toISOString(),
      error: truncate(error instanceof Error ? error.message : String(error), 64 * 1024),
    };
    if (args.keepJson) {
      await writeJson(outputPath, result);
    }
    const rendered = await renderPackageOutput(args, pkg, result);
    await writer.write(index, rendered);
    if (!args.keepJson) {
      await fs.rm(outputPath, { force: true });
    }
    row = summarizePackageResult(pkg, outputPath, result, false, rendered);
    logProgress(`[pkg ${progress}] ${pkg.displayId} failed`);
  }

  return row;
}

function summarizePackageResult(pkg, outputPath, result, skipped, rendered) {
  if (result.status !== "ok") {
    return {
      ...pkg,
      status: result.status,
      outputPath,
      htmlPath: rendered?.htmlPath,
      error: result.error,
      skipped,
    };
  }

  const counts = countScopeGraph(result.scopeGraph);
  return {
    ...pkg,
    status: "ok",
    outputPath: result.outputPath ?? outputPath,
    htmlPath: rendered?.htmlPath,
    skipped,
    elapsedMs: result.elapsedMs,
    maxRssKiB: result.maxRssKiB,
    ...counts,
  };
}

function countScopeGraph(scopeGraph) {
  const counts = {
    files: 0,
    scopes: 0,
    fileScopes: 0,
    functionScopes: 0,
    variables: 0,
    typedVariables: 0,
    typeMappings: 0,
  };

  for (const file of scopeGraph?.files || []) {
    counts.files += 1;
    counts.typeMappings += file.typeMappings?.length || 0;
    for (const scope of file.scopes || []) {
      counts.scopes += 1;
      if (scope.kind === "file") {
        counts.fileScopes += 1;
      } else if (scope.kind === "function") {
        counts.functionScopes += 1;
      }
      for (const variable of scope.variables || []) {
        counts.variables += 1;
        if (variable.ty) {
          counts.typedVariables += 1;
        }
      }
    }
  }

  return counts;
}

class OrderedOutputWriter {
  constructor(args) {
    this.args = args;
    this.nextIndex = 0;
    this.pending = new Map();
    this.chain = Promise.resolve();
    this.scopeGraphPath = path.join(args.out, "scope-graph.txt");
    this.typeMappingsPath = path.join(args.out, "type-mappings.tsv");
  }

  async init() {
    await fs.mkdir(this.args.out, { recursive: true });
    if (!this.args.keepJson) {
      await fs.rm(path.join(this.args.out, "packages"), { recursive: true, force: true });
    }
    if (this.args.html) {
      await fs.rm(path.join(this.args.out, "html"), { recursive: true, force: true });
      await fs.mkdir(path.join(this.args.out, "html"), { recursive: true });
    } else {
      await fs.rm(path.join(this.args.out, "index.html"), { force: true });
    }
    await fs.rm(path.join(this.args.out, "results.json"), { force: true });
    await fs.rm(path.join(this.args.out, "results.jsonl"), { force: true });
    await fs.writeFile(this.scopeGraphPath, "# Tinymist Package Tyck Scope Graph\n\n");
    await fs.writeFile(this.typeMappingsPath, "package\tfile\trange\ttype\n");
  }

  async write(index, rendered) {
    this.chain = this.chain.then(async () => {
      this.pending.set(index, rendered);
      while (this.pending.has(this.nextIndex)) {
        const next = this.pending.get(this.nextIndex);
        this.pending.delete(this.nextIndex);
        await fs.appendFile(this.scopeGraphPath, next.scopeText);
        if (next.typeMappingsText) {
          await fs.appendFile(this.typeMappingsPath, next.typeMappingsText);
        }
        this.nextIndex += 1;
      }
    });
    return this.chain;
  }

  async close() {
    await this.chain;
  }
}

async function renderPackageOutput(args, pkg, result) {
  const htmlPath = args.html ? await writePackageHtml(args, pkg, result) : undefined;

  if (result.status !== "ok") {
    return {
      htmlPath,
      scopeText: [
        `package ${pkg.spec}`,
        `  failed ${oneLine(result.error || "unknown error")}`,
        "",
      ].join("\n"),
      typeMappingsText: "",
    };
  }

  return {
    htmlPath,
    scopeText: renderScopeGraph(args, pkg, result.scopeGraph),
    typeMappingsText: renderTypeMappings(args, pkg, result.scopeGraph),
  };
}

async function writePackageHtml(args, pkg, result) {
  const relativePath = htmlRelativePath(pkg);
  const outputPath = path.join(args.out, relativePath);
  const html =
    result.status === "ok"
      ? await renderPackageHtml(args, pkg, result.scopeGraph)
      : renderFailedPackageHtml(pkg, result);
  await fs.mkdir(path.dirname(outputPath), { recursive: true });
  await fs.writeFile(outputPath, html);
  return relativePath;
}

function renderScopeGraph(args, pkg, scopeGraph) {
  const lines = [`package ${pkg.spec}`, `  entry ${scopeGraph.entrypoint?.fileId || "-"}`];

  for (const file of scopeGraph.files || []) {
    lines.push(`  file ${file.path}`);
    for (const imported of file.imports || []) {
      lines.push(`    import ${imported.fileId}`);
    }
    for (const scope of file.scopes || []) {
      const declRange = scope.declaration ? formatRange(scope.declaration.range) : "-";
      lines.push(
        `    scope ${scope.kind} ${oneLine(scope.name)} range=${declRange} vars=${scope.variables?.length || 0}`,
      );
      for (const variable of scope.variables || []) {
        lines.push(
          [
            "      var",
            variable.source,
            variable.kind,
            oneLine(variable.name),
            `range=${formatRange(variable.declaration?.range)}`,
            `type=${typeText(variable.ty, args.maxCellChars)}`,
            `decl=${oneLine(variable.declaration?.debug || "-")}`,
          ].join(" "),
        );
      }
    }
  }

  lines.push("");
  return `${lines.join("\n")}\n`;
}

function renderTypeMappings(args, pkg, scopeGraph) {
  const lines = [];
  for (const file of scopeGraph.files || []) {
    for (const mapping of file.typeMappings || []) {
      lines.push(
        [
          pkg.spec,
          file.path,
          formatRange(mapping.range),
          typeText(mapping.ty, args.maxCellChars),
        ].join("\t"),
      );
    }
  }
  return lines.length === 0 ? "" : `${lines.join("\n")}\n`;
}

async function renderPackageHtml(args, pkg, scopeGraph) {
  const sourceIndex = new SourceLineIndex(args, pkg);
  const files = [];

  for (const file of scopeGraph.files || []) {
    const fileHref = await sourceIndex.githubHref(file.path, { start: 0 });
    const sections = [
      `<section class="file">`,
      `<h2><a href="${attr(fileHref.href)}">${escapeHtml(file.path)}<span>L${fileHref.line}</span></a></h2>`,
    ];

    if (file.imports?.length) {
      sections.push(`<details class="imports"><summary>Imports (${file.imports.length})</summary>`);
      sections.push(`<ul>`);
      for (const imported of file.imports) {
        sections.push(`<li><code>${escapeHtml(imported.fileId)}</code></li>`);
      }
      sections.push(`</ul></details>`);
    }

    for (const scope of file.scopes || []) {
      sections.push(await renderScopeHtml(args, sourceIndex, file, scope));
    }

    sections.push(`</section>`);
    files.push(sections.join("\n"));
  }

  return htmlDocument(
    `${pkg.spec} Tyck Scope Graph`,
    `<main>
  <header class="page-header">
    <a class="back" href="../index.html">All packages</a>
    <h1>${escapeHtml(pkg.spec)}</h1>
    <dl>
      <div><dt>Entrypoint</dt><dd><code>${escapeHtml(scopeGraph.entrypoint?.fileId || "-")}</code></dd></div>
      <div><dt>Files</dt><dd>${scopeGraph.files?.length || 0}</dd></div>
    </dl>
  </header>
  ${files.join("\n")}
</main>`,
  );
}

async function renderScopeHtml(args, sourceIndex, file, scope) {
  const range = scope.declaration?.range || { start: 0 };
  const href = await sourceIndex.githubHref(file.path, range);
  const variables = scope.variables || [];
  const rows = [];

  for (const variable of variables) {
    const variableHref = await sourceIndex.githubHref(file.path, variable.declaration?.range);
    rows.push(`<tr>
  <td><a href="${attr(variableHref.href)}">L${variableHref.line}</a></td>
  <td>${escapeHtml(variable.source)}</td>
  <td>${escapeHtml(variable.kind)}</td>
  <td><code>${escapeHtml(oneLine(variable.name, args.maxCellChars))}</code></td>
  <td><code>${escapeHtml(formatRange(variable.declaration?.range))}</code></td>
  <td><code>${escapeHtml(typeText(variable.ty, args.maxCellChars))}</code></td>
  <td><code>${escapeHtml(oneLine(variable.declaration?.debug || "-", args.maxCellChars))}</code></td>
</tr>`);
  }

  return `<section class="scope">
  <h3>
    <a class="line-link" href="${attr(href.href)}">L${href.line}</a>
    <span>${escapeHtml(scope.kind)}</span>
    <code>${escapeHtml(oneLine(scope.name, args.maxCellChars))}</code>
    <small>${escapeHtml(formatRange(scope.declaration?.range))}</small>
  </h3>
  <table>
    <thead>
      <tr><th>Line</th><th>Source</th><th>Kind</th><th>Name</th><th>Range</th><th>Type</th><th>Decl</th></tr>
    </thead>
    <tbody>
      ${rows.join("\n") || `<tr><td colspan="7" class="empty">No variables</td></tr>`}
    </tbody>
  </table>
</section>`;
}

function renderFailedPackageHtml(pkg, result) {
  return htmlDocument(
    `${pkg.spec} Tyck Scope Graph Failed`,
    `<main>
  <header class="page-header">
    <a class="back" href="../index.html">All packages</a>
    <h1>${escapeHtml(pkg.spec)}</h1>
  </header>
  <section class="file">
    <h2>Failed</h2>
    <pre>${escapeHtml(result.error || "unknown error")}</pre>
  </section>
</main>`,
  );
}

class SourceLineIndex {
  constructor(args, pkg) {
    this.args = args;
    this.pkg = pkg;
    this.cache = new Map();
  }

  async githubHref(filePath, range) {
    const line = await this.lineFor(filePath, range?.start || 0);
    return {
      line,
      href: githubSourceHref(this.args, this.pkg, filePath, line),
    };
  }

  async lineFor(filePath, byteOffset) {
    const index = await this.lineStarts(filePath);
    const offset = Math.max(0, Math.min(byteOffset, index.lengthBytes));
    return upperBound(index.starts, offset);
  }

  async lineStarts(filePath) {
    if (this.cache.has(filePath)) {
      return this.cache.get(filePath);
    }

    const sourcePath = path.join(packageDir(this.args.packageCachePath, this.pkg), filePath);
    let buffer;
    try {
      buffer = await fs.readFile(sourcePath);
    } catch {
      buffer = Buffer.alloc(0);
    }

    const starts = [0];
    for (let index = 0; index < buffer.length; index += 1) {
      if (buffer[index] === 0x0a) {
        starts.push(index + 1);
      }
    }

    const entry = {
      starts,
      lengthBytes: buffer.length,
    };
    this.cache.set(filePath, entry);
    return entry;
  }
}

function upperBound(values, needle) {
  let left = 0;
  let right = values.length;
  while (left < right) {
    const mid = Math.floor((left + right) / 2);
    if (values[mid] <= needle) {
      left = mid + 1;
    } else {
      right = mid;
    }
  }
  return Math.max(1, left);
}

function githubSourceHref(args, pkg, filePath, line) {
  const base = args.githubBaseUrl.replace(/\/+$/, "");
  const relativePath = [pkg.namespace, pkg.name, pkg.version, ...String(filePath).split("/")]
    .map(encodeURIComponent)
    .join("/");
  return `${base}/${relativePath}#L${line}`;
}

function htmlDocument(title, body) {
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escapeHtml(title)}</title>
  <style>
    :root { color-scheme: light dark; --bg: #f7f8fa; --fg: #1f2328; --muted: #59636e; --border: #d0d7de; --panel: #ffffff; --accent: #0969da; }
    @media (prefers-color-scheme: dark) { :root { --bg: #0d1117; --fg: #e6edf3; --muted: #8b949e; --border: #30363d; --panel: #161b22; --accent: #58a6ff; } }
    * { box-sizing: border-box; }
    body { margin: 0; background: var(--bg); color: var(--fg); font: 14px/1.45 system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    main { width: min(1400px, calc(100vw - 32px)); margin: 24px auto 40px; }
    a { color: var(--accent); text-decoration: none; }
    a:hover { text-decoration: underline; }
    code, pre { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace; }
    .page-header { margin-bottom: 18px; }
    .back { display: inline-block; margin-bottom: 8px; }
    h1 { margin: 0 0 12px; font-size: 24px; line-height: 1.2; }
    h2 { margin: 0; font-size: 17px; }
    h2 a { display: inline-flex; gap: 8px; align-items: baseline; }
    h2 span, .line-link { font-size: 12px; font-weight: 600; }
    dl { display: flex; flex-wrap: wrap; gap: 12px 24px; margin: 0; color: var(--muted); }
    dl div { display: flex; gap: 6px; }
    dt { font-weight: 600; }
    dd { margin: 0; color: var(--fg); }
    .file { margin: 18px 0; padding: 14px; border: 1px solid var(--border); border-radius: 8px; background: var(--panel); }
    .imports { margin: 10px 0; color: var(--muted); }
    .imports ul { margin: 8px 0 0; padding-left: 22px; }
    .scope { margin-top: 14px; border-top: 1px solid var(--border); padding-top: 12px; }
    .scope h3 { display: flex; gap: 8px; align-items: baseline; margin: 0 0 8px; font-size: 14px; }
    .scope h3 span { color: var(--muted); text-transform: uppercase; font-size: 11px; letter-spacing: .04em; }
    .scope h3 small { color: var(--muted); font-weight: 400; }
    table { width: 100%; border-collapse: collapse; table-layout: fixed; }
    th, td { border-top: 1px solid var(--border); padding: 6px 8px; text-align: left; vertical-align: top; overflow-wrap: anywhere; }
    th { color: var(--muted); font-size: 12px; font-weight: 600; }
    td:first-child, th:first-child { width: 64px; }
    td:nth-child(2), th:nth-child(2), td:nth-child(3), th:nth-child(3) { width: 96px; }
    td:nth-child(5), th:nth-child(5) { width: 110px; }
    .empty { color: var(--muted); text-align: center; }
    pre { overflow: auto; padding: 12px; border: 1px solid var(--border); border-radius: 6px; }
  </style>
</head>
<body>
${body}
</body>
</html>
`;
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

function formatRange(range) {
  if (!range) {
    return "-";
  }
  return `${range.start}..${range.end}`;
}

function typeText(ty, maxLength = DEFAULT_MAX_CELL_CHARS) {
  if (!ty) {
    return "?";
  }
  return oneLine(ty.describe || ty.repr || ty.debug || "?", maxLength);
}

function oneLine(value, maxLength = DEFAULT_MAX_CELL_CHARS) {
  const input = String(value);
  const limit = Math.max(1, maxLength || DEFAULT_MAX_CELL_CHARS);
  let output = "";
  let pendingSpace = false;
  let truncated = false;

  for (let index = 0; index < input.length; index += 1) {
    const code = input.charCodeAt(index);
    if (isAsciiWhitespace(code)) {
      pendingSpace = output.length > 0;
      continue;
    }

    if (pendingSpace) {
      if (output.length >= limit) {
        truncated = true;
        break;
      }
      output += " ";
      pendingSpace = false;
    }

    if (output.length >= limit) {
      truncated = true;
      break;
    }
    output += input[index];
  }

  if (truncated) {
    return `${output} ... truncated (${input.length} chars) ...`;
  }
  return output;
}

function isAsciiWhitespace(code) {
  return (
    code === 0x20 ||
    code === 0x09 ||
    code === 0x0a ||
    code === 0x0b ||
    code === 0x0c ||
    code === 0x0d
  );
}

async function run(command, commandArgs, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, commandArgs, {
      cwd: options.cwd,
      env: options.env || process.env,
      stdio: ["ignore", "pipe", "pipe"],
      detached: process.platform !== "win32",
    });

    activeChildren.add(child);

    let stdout = "";
    let stderr = "";
    let timedOut = false;
    let settled = false;
    let timeout;
    let forceKillTimeout;

    const finish = (error, result) => {
      if (settled) {
        return;
      }
      settled = true;
      activeChildren.delete(child);
      if (timeout) {
        clearTimeout(timeout);
      }
      if (forceKillTimeout) {
        clearTimeout(forceKillTimeout);
      }
      if (error) {
        reject(error);
      } else {
        resolve(result);
      }
    };

    if (Number.isFinite(options.timeoutMs) && options.timeoutMs > 0) {
      timeout = setTimeout(() => {
        timedOut = true;
        terminateChild(child, "SIGTERM");
        forceKillTimeout = setTimeout(() => {
          terminateChild(child, "SIGKILL");
        }, 5000);
        forceKillTimeout.unref?.();
      }, options.timeoutMs);
      timeout.unref?.();
    }

    child.stdout?.on("data", (chunk) => {
      stdout = appendLimited(stdout, chunk.toString());
    });
    child.stderr?.on("data", (chunk) => {
      stderr = appendLimited(stderr, chunk.toString());
    });
    child.on("error", (error) => finish(error));
    child.on("close", (code) => {
      if (timedOut) {
        finish(
          new Error(
            [
              `${command} ${commandArgs.join(" ")} timed out after ${formatMs(options.timeoutMs)}`,
              stdout.trim() && `stdout:\n${stdout.trim()}`,
              stderr.trim() && `stderr:\n${stderr.trim()}`,
            ]
              .filter(Boolean)
              .join("\n\n"),
          ),
        );
        return;
      }
      if (code === 0) {
        finish(null, { stdout, stderr });
      } else {
        finish(
          new Error(
            [
              `${command} ${commandArgs.join(" ")} failed with exit code ${code}`,
              stdout.trim() && `stdout:\n${stdout.trim()}`,
              stderr.trim() && `stderr:\n${stderr.trim()}`,
            ]
              .filter(Boolean)
              .join("\n\n"),
          ),
        );
      }
    });
  });
}

function appendLimited(current, chunk) {
  const next = current + chunk;
  if (Buffer.byteLength(next) <= OUTPUT_LIMIT_BYTES) {
    return next;
  }
  return next.slice(-OUTPUT_LIMIT_BYTES);
}

function terminateChild(child, signal) {
  if (!child.pid) {
    return;
  }
  try {
    if (process.platform !== "win32") {
      process.kill(-child.pid, signal);
    } else {
      child.kill(signal);
    }
  } catch {
    try {
      child.kill(signal);
    } catch {
      // The process may have already exited.
    }
  }
}

async function runMeasured(command, commandArgs, options = {}) {
  const canMeasureMemory = process.platform !== "win32" && (await exists("/usr/bin/time"));
  const marker = "TINYMIST_TYCK_PACKAGE_SCAN_MAX_RSS_KIB=";
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

async function writeJson(filePath, value) {
  await fs.mkdir(path.dirname(filePath), { recursive: true });
  await fs.writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

async function writeAggregate(args, rows, packages) {
  const successful = rows.filter((row) => row.status === "ok");
  const failed = rows.filter((row) => row.status === "failed");
  const skipped = rows.filter((row) => row.skipped);
  const summary = {
    schema: 1,
    packageCachePath: args.packageCachePath,
    packagePath: args.packagePath,
    total: rows.length,
    discovered: packages.length,
    ok: successful.length,
    failed: failed.length,
    skipped: skipped.length,
    totalElapsedMs: sumDefined(successful, "elapsedMs"),
    totalFiles: sumDefined(successful, "files"),
    totalScopes: sumDefined(successful, "scopes"),
    totalFileScopes: sumDefined(successful, "fileScopes"),
    totalFunctionScopes: sumDefined(successful, "functionScopes"),
    totalVariables: sumDefined(successful, "variables"),
    totalTypedVariables: sumDefined(successful, "typedVariables"),
    totalTypeMappings: sumDefined(successful, "typeMappings"),
  };

  await writeJson(path.join(args.out, "summary.json"), summary);
  await fs.writeFile(path.join(args.out, "packages.tsv"), renderPackagesTsv(rows));
  if (args.html) {
    await fs.writeFile(path.join(args.out, "index.html"), renderIndexHtml(summary, rows));
  }

  return summary;
}

function renderPackagesTsv(rows) {
  const header = [
    "package",
    "status",
    "files",
    "scopes",
    "fileScopes",
    "functionScopes",
    "variables",
    "typedVariables",
    "typeMappings",
    "elapsedMs",
    "maxRssKiB",
    "html",
    "error",
  ];
  const body = rows.map((row) =>
    [
      row.spec,
      row.status,
      row.files ?? "",
      row.scopes ?? "",
      row.fileScopes ?? "",
      row.functionScopes ?? "",
      row.variables ?? "",
      row.typedVariables ?? "",
      row.typeMappings ?? "",
      row.elapsedMs?.toFixed?.(2) ?? "",
      row.maxRssKiB ?? "",
      row.htmlPath ?? "",
      row.error ? tsvCell(row.error.split(/\r?\n/, 1)[0]) : "",
    ].join("\t"),
  );
  return `${header.join("\t")}\n${body.join("\n")}\n`;
}

function renderIndexHtml(summary, rows) {
  const bodyRows = rows
    .map((row) => {
      const packageCell = row.htmlPath
        ? `<a href="${attr(row.htmlPath)}"><code>${escapeHtml(row.spec)}</code></a>`
        : `<code>${escapeHtml(row.spec)}</code>`;
      return `<tr>
  <td>${packageCell}</td>
  <td>${escapeHtml(row.status)}</td>
  <td>${row.files ?? ""}</td>
  <td>${row.scopes ?? ""}</td>
  <td>${row.functionScopes ?? ""}</td>
  <td>${row.variables ?? ""}</td>
  <td>${row.typedVariables ?? ""}</td>
  <td>${row.typeMappings ?? ""}</td>
  <td>${row.elapsedMs?.toFixed?.(2) ?? ""}</td>
  <td>${row.error ? `<code>${escapeHtml(row.error.split(/\r?\n/, 1)[0])}</code>` : ""}</td>
</tr>`;
    })
    .join("\n");

  return htmlDocument(
    "Tinymist Package Tyck Scope Graph",
    `<main>
  <header class="page-header">
    <h1>Tinymist Package Tyck Scope Graph</h1>
    <dl>
      <div><dt>Packages</dt><dd>${summary.total}</dd></div>
      <div><dt>OK</dt><dd>${summary.ok}</dd></div>
      <div><dt>Failed</dt><dd>${summary.failed}</dd></div>
      <div><dt>Files</dt><dd>${summary.totalFiles}</dd></div>
      <div><dt>Scopes</dt><dd>${summary.totalScopes}</dd></div>
      <div><dt>Variables</dt><dd>${summary.totalVariables}</dd></div>
      <div><dt>Type mappings</dt><dd>${summary.totalTypeMappings}</dd></div>
    </dl>
  </header>
  <section class="file">
    <h2>Packages</h2>
    <table>
      <thead>
        <tr><th>Package</th><th>Status</th><th>Files</th><th>Scopes</th><th>Functions</th><th>Vars</th><th>Typed Vars</th><th>Mappings</th><th>Elapsed ms</th><th>Error</th></tr>
      </thead>
      <tbody>
        ${bodyRows}
      </tbody>
    </table>
  </section>
</main>`,
  );
}

function tsvCell(value) {
  return oneLine(value).replaceAll("\t", " ");
}

function sumDefined(rows, key) {
  return rows.reduce((sum, row) => sum + (Number.isFinite(row[key]) ? row[key] : 0), 0);
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

function truncate(value, maxLength) {
  if (value.length <= maxLength) {
    return value;
  }
  return `${value.slice(0, maxLength)}\n... truncated ...`;
}

function logProgress(message) {
  process.stdout.write(`${message}\n`);
}

function installSignalHandlers() {
  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.once(signal, () => {
      for (const child of activeChildren) {
        terminateChild(child, "SIGTERM");
      }
      setTimeout(() => {
        for (const child of activeChildren) {
          terminateChild(child, "SIGKILL");
        }
        process.exit(signal === "SIGINT" ? 130 : 143);
      }, 500).unref?.();
    });
  }
}

async function main() {
  installSignalHandlers();
  const args = parseArgs(process.argv.slice(2));

  if (isPathLike(args.tinymist) && !(await exists(args.tinymist))) {
    throw new Error(`Tinymist binary does not exist: ${args.tinymist}`);
  }

  await fs.mkdir(args.out, { recursive: true });
  const context = await prepareWorkspace(args);
  const writer = new OrderedOutputWriter(args);
  await writer.init();

  console.log(`Scanning packages in ${args.packageCachePath}`);
  let packages = await discoverPackages(args);
  if (packages.length === 0) {
    throw new Error(`No Typst packages found in ${args.packageCachePath}`);
  }
  if (args.limit !== undefined) {
    packages = packages.slice(0, args.limit);
  }

  console.log(`Tinymist: ${args.tinymist}`);
  console.log(`Found ${packages.length} package version(s)`);
  console.log(`Running with ${args.jobs} parallel job(s)`);

  let rows;
  try {
    rows = await mapLimit(packages, args.jobs, async (pkg, index) => {
      const progress = `${index + 1}/${packages.length}`;
      return scanPackage(args, context, pkg, progress, index, writer);
    });
  } finally {
    await writer.close();
    await fs.rm(path.join(args.out, ".tmp"), { recursive: true, force: true });
  }

  const summary = await writeAggregate(args, rows, packages);
  console.log(`Summary written to ${path.join(args.out, "summary.json")}`);
  console.log(`Scope graph written to ${path.join(args.out, "scope-graph.txt")}`);
  console.log(`Type mappings written to ${path.join(args.out, "type-mappings.tsv")}`);
  console.log(`Package stats written to ${path.join(args.out, "packages.tsv")}`);
  if (args.html) {
    console.log(`HTML index written to ${path.join(args.out, "index.html")}`);
  }

  if (summary.failed > 0) {
    console.error(`${summary.failed} package(s) failed`);
    for (const row of rows.filter((item) => item.status === "failed")) {
      console.error(`- ${row.displayId}: ${row.error.split(/\r?\n/, 1)[0]}`);
    }
    if (!args.allowFailures) {
      process.exitCode = 1;
    }
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : error);
  process.exit(1);
});
