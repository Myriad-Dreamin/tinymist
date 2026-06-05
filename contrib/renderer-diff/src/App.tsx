import { useEffect, useMemo, useRef, useState } from "react";
import {
  artifactProxyDownloadUrl,
  downloadArtifactZip,
  listRendererDiffArtifacts,
  parseActionRunUrl,
  type ActionRunRef,
  type GithubArtifact,
} from "./github";
import type {
  LoadedRendererDiffBundle,
  RendererDiffCase,
  RendererDiffComparison,
  RendererDiffGroup,
} from "./types";
import { loadRendererDiffZip, revokeRendererDiffBundle } from "./zip";

type StatusFilter = "all" | "different" | "render-error" | "matched";

interface LoadMessage {
  kind: "info" | "error";
  text: string;
}

interface ArtifactDownload {
  name: string;
  href: string;
  sizeInBytes: number;
}

interface LoadProgress {
  current: number;
  total: number;
  label: string;
  detail: string;
}

export function App() {
  const [runUrl, setRunUrl] = useState(() => initialActionRunUrl());
  const autoLoadRunUrl = useRef(runUrl);
  const [bundles, setBundles] = useState<LoadedRendererDiffBundle[]>([]);
  const [artifactDownloads, setArtifactDownloads] = useState<ArtifactDownload[]>([]);
  const [selectedArtifact, setSelectedArtifact] = useState(0);
  const [selectedCase, setSelectedCase] = useState("");
  const [lhsGroup, setLhsGroup] = useState("");
  const [rhsGroup, setRhsGroup] = useState("");
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [message, setMessage] = useState<LoadMessage>({
    kind: "info",
    text: "No renderer diff bundle loaded.",
  });
  const [loadProgress, setLoadProgress] = useState<LoadProgress | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    return () => {
      bundles.forEach(revokeRendererDiffBundle);
    };
  }, [bundles]);

  const activeBundle = bundles[selectedArtifact] ?? null;
  const groupPair = useMemo(
    () => selectedGroupPair(activeBundle, lhsGroup, rhsGroup),
    [activeBundle, lhsGroup, rhsGroup],
  );

  useEffect(() => {
    if (!activeBundle) {
      return;
    }
    const [lhs, rhs] = defaultGroupPair(activeBundle);
    if (!activeBundle.manifest.groups.some((group) => group.id === lhsGroup)) {
      setLhsGroup(lhs);
    }
    if (!activeBundle.manifest.groups.some((group) => group.id === rhsGroup)) {
      setRhsGroup(rhs);
    }
  }, [activeBundle, lhsGroup, rhsGroup]);

  const visibleCases = useMemo(() => {
    if (!activeBundle || !groupPair) {
      return [];
    }

    const [lhs, rhs] = groupPair;
    const needle = query.trim().toLowerCase();
    return activeBundle.manifest.cases
      .filter((item) => hasGroupAssets(item, lhs, rhs))
      .filter((item) => statusFilter === "all" || caseStatus(item, lhs, rhs) === statusFilter)
      .filter((item) => !needle || item.name.toLowerCase().includes(needle))
      .sort((left, right) => compareCases(left, right, lhs, rhs));
  }, [activeBundle, groupPair, query, statusFilter]);

  const activeCase = useMemo(() => {
    if (!visibleCases.length) {
      return null;
    }

    return visibleCases.find((item) => item.name === selectedCase) ?? visibleCases[0];
  }, [selectedCase, visibleCases]);

  useEffect(() => {
    if (activeCase) {
      setSelectedCase(activeCase.name);
    }
  }, [activeCase]);

  useEffect(() => {
    const url = autoLoadRunUrl.current;
    if (!url) {
      return;
    }
    autoLoadRunUrl.current = "";
    void loadFromAction(url);
  }, []);

  async function replaceBundles(nextBundles: LoadedRendererDiffBundle[]) {
    setBundles((previous) => {
      previous.forEach(revokeRendererDiffBundle);
      return nextBundles;
    });
    setSelectedArtifact(0);
    setSelectedCase(nextBundles[0]?.manifest.cases[0]?.name ?? "");
    const [lhs, rhs] = nextBundles[0] ? defaultGroupPair(nextBundles[0]) : ["", ""];
    setLhsGroup(lhs);
    setRhsGroup(rhs);
  }

  async function loadFromAction(input = runUrl) {
    const ref = parseActionRunUrl(input);
    if (!ref) {
      setArtifactDownloads([]);
      setMessage({ kind: "error", text: "The action URL is not valid." });
      return;
    }

    const canonicalRunUrl = actionRunUrl(ref);
    setRunUrl(canonicalRunUrl);
    writeActionRunParams(ref);
    setIsLoading(true);
    setLoadProgress(null);
    setArtifactDownloads([]);
    setMessage({ kind: "info", text: "Loading GitHub Actions artifacts..." });

    try {
      const artifacts = await listRendererDiffArtifacts(ref);
      const available = artifacts.filter((artifact) => !artifact.expired);

      if (!available.length) {
        setMessage({ kind: "error", text: "No active renderer-diff-* artifacts were found." });
        setArtifactDownloads([]);
        await replaceBundles([]);
        return;
      }

      const downloads = available.map((artifact) => ({
        name: artifact.name,
        href: artifactProxyDownloadUrl(ref, artifact),
        sizeInBytes: artifact.size_in_bytes,
      }));
      setArtifactDownloads(downloads);

      setMessage({ kind: "info", text: "Loading renderer diff bundles..." });
      setLoadProgress({
        current: 0,
        total: available.length,
        label: "Loading renderer diff bundles",
        detail: available[0]?.name ?? "",
      });
      const loaded = await loadArtifacts(ref, available, setLoadProgress);
      await replaceBundles(loaded.bundles);
      setMessage({
        kind: loaded.failures.length ? "error" : "info",
        text: loadSummaryMessage(loaded.bundles.length, loaded.failures),
      });
    } catch (error) {
      setArtifactDownloads([]);
      setMessage({ kind: "error", text: errorMessage(error) });
    } finally {
      setLoadProgress(null);
      setIsLoading(false);
    }
  }

  async function loadLocalFiles(files: FileList | null) {
    if (!files?.length) {
      return;
    }

    setIsLoading(true);
    setLoadProgress(null);
    setMessage({ kind: "info", text: "Loading local renderer diff ZIP files..." });

    try {
      const loaded: LoadedRendererDiffBundle[] = [];
      const failures: string[] = [];
      const localFiles = Array.from(files);

      for (const [index, file] of localFiles.entries()) {
        setLoadProgress({
          current: index,
          total: localFiles.length,
          label: "Loading local ZIP files",
          detail: file.name,
        });
        try {
          loaded.push(await loadRendererDiffZip(file.name, file));
        } catch (error) {
          failures.push(`${file.name}: ${errorMessage(error)}`);
        }
        setLoadProgress({
          current: index + 1,
          total: localFiles.length,
          label: "Loading local ZIP files",
          detail: file.name,
        });
      }

      await replaceBundles(loaded);
      setMessage({
        kind: failures.length ? "error" : "info",
        text: failures.length
          ? `Loaded ${loaded.length} bundle(s). ${failures.join(" ")}`
          : `Loaded ${loaded.length} local bundle(s).`,
      });
    } finally {
      setLoadProgress(null);
      setIsLoading(false);
    }
  }

  const pairSummary = useMemo(() => {
    if (!activeBundle || !groupPair) {
      return { total: 0, matched: 0, different: 0, renderErrors: 0 };
    }
    return summarizePair(activeBundle.manifest.cases, groupPair[0], groupPair[1]);
  }, [activeBundle, groupPair]);

  return (
    <main className="app-shell">
      <header className="topbar">
        <div>
          <h1>Renderer Diff</h1>
          <p>
            {activeBundle && groupPair
              ? `${groupLabel(activeBundle, groupPair[0])} vs ${groupLabel(activeBundle, groupPair[1])}`
              : "Tinymist renderer protocol viewer"}
          </p>
        </div>
        <div className="actions">
          <input
            value={runUrl}
            onChange={(event) => setRunUrl(event.target.value)}
            placeholder="https://github.com/owner/repo/actions/runs/123"
            aria-label="GitHub Actions run URL"
          />
          <button onClick={() => void loadFromAction()} disabled={isLoading}>
            Load
          </button>
          <label className="file-button">
            ZIP
            <input
              type="file"
              accept=".zip,application/zip"
              multiple
              onChange={(event) => void loadLocalFiles(event.target.files)}
            />
          </label>
        </div>
      </header>

      <section className={`message ${message.kind}`}>{message.text}</section>

      {loadProgress && <LoadProgressBar progress={loadProgress} />}

      {artifactDownloads.length > 0 && (
        <section className="download-strip" aria-label="Renderer diff artifact downloads">
          {artifactDownloads.map((artifact) => (
            <a key={artifact.href} href={artifact.href} target="_blank" rel="noreferrer">
              <span>{artifact.name}</span>
              <strong>{formatBytes(artifact.sizeInBytes)}</strong>
            </a>
          ))}
        </section>
      )}

      <section className="summary-strip" aria-label="Renderer diff summary">
        <Metric label="Artifacts" value={bundles.length} />
        <Metric label="Cases" value={pairSummary.total} />
        <Metric label="Different" value={pairSummary.different} />
        <Metric label="Errors" value={pairSummary.renderErrors} />
        <Metric label="Matched" value={pairSummary.matched} />
      </section>

      {bundles.length > 0 && (
        <nav className="artifact-tabs" aria-label="Renderer diff artifacts">
          {bundles.map((bundle, index) => (
            <button
              key={bundle.artifactName}
              className={index === selectedArtifact ? "selected" : ""}
              onClick={() => {
                setSelectedArtifact(index);
                setSelectedCase(bundle.manifest.cases[0]?.name ?? "");
                const [lhs, rhs] = defaultGroupPair(bundle);
                setLhsGroup(lhs);
                setRhsGroup(rhs);
              }}
            >
              {bundle.artifactName}
            </button>
          ))}
        </nav>
      )}

      <section className="workspace">
        <aside className="case-panel">
          <div className="filters">
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Filter cases"
              aria-label="Filter cases"
            />
            {activeBundle && (
              <div className="group-selectors">
                <GroupSelect
                  label="Left"
                  groups={activeBundle.manifest.groups}
                  value={groupPair?.[0] ?? ""}
                  onChange={setLhsGroup}
                />
                <GroupSelect
                  label="Right"
                  groups={activeBundle.manifest.groups}
                  value={groupPair?.[1] ?? ""}
                  onChange={setRhsGroup}
                />
              </div>
            )}
            <div className="segments" role="group" aria-label="Status filter">
              {(["all", "different", "render-error", "matched"] as const).map((item) => (
                <button
                  key={item}
                  className={statusFilter === item ? "selected" : ""}
                  onClick={() => setStatusFilter(item)}
                >
                  {filterLabel(item)}
                </button>
              ))}
            </div>
          </div>

          <div className="case-list" aria-label="Renderer diff cases">
            {visibleCases.map((item) => {
              const comparison = groupPair
                ? comparisonFor(item, groupPair[0], groupPair[1])
                : undefined;
              return (
                <button
                  key={item.name}
                  className={item.name === activeCase?.name ? "case-row selected" : "case-row"}
                  onClick={() => setSelectedCase(item.name)}
                >
                  <span
                    className={`status-dot ${statusClass(caseStatusFromComparison(item, comparison))}`}
                  />
                  <span className="case-name">{item.name}</span>
                  <span className="case-score">
                    {comparison ? formatPercent(comparison.metrics.pixelMismatchRatio) : "-"}
                  </span>
                  <span className="case-distance">
                    {comparison?.metrics.perceptualHashDistance ?? "-"}
                  </span>
                </button>
              );
            })}
            {!visibleCases.length && (
              <div className="empty">No cases match the current filter.</div>
            )}
          </div>
        </aside>

        <section className="detail-panel">
          {activeBundle && activeCase && groupPair ? (
            <CaseDetail
              bundle={activeBundle}
              item={activeCase}
              lhs={groupPair[0]}
              rhs={groupPair[1]}
            />
          ) : (
            <div className="empty detail-empty">No renderer diff case selected.</div>
          )}
        </section>
      </section>
    </main>
  );
}

function GroupSelect({
  label,
  groups,
  value,
  onChange,
}: {
  label: string;
  groups: RendererDiffGroup[];
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label>
      <span>{label}</span>
      <select value={value} onChange={(event) => onChange(event.target.value)}>
        {groups.map((group) => (
          <option key={group.id} value={group.id}>
            {group.label}
          </option>
        ))}
      </select>
    </label>
  );
}

function LoadProgressBar({ progress }: { progress: LoadProgress }) {
  const percent = progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0;
  return (
    <section className="load-progress" aria-label="Bundle loading progress">
      <div>
        <span>{progress.label}</span>
        <strong>
          {progress.current}/{progress.total}
        </strong>
      </div>
      <progress value={progress.current} max={progress.total} />
      <p>
        {percent}% {progress.detail}
      </p>
    </section>
  );
}

function initialActionRunUrl(): string {
  const params = new URLSearchParams(window.location.search);
  const explicitUrl = params.get("url") ?? params.get("actionUrl") ?? params.get("action") ?? "";
  if (explicitUrl) {
    return explicitUrl;
  }

  const run = params.get("run") ?? params.get("runId");
  if (!run) {
    return "";
  }

  if (run.startsWith("https://github.com/")) {
    return run;
  }

  const owner = params.get("owner");
  const repo = params.get("repo");
  if (!owner || !repo || !/^\d+$/.test(run)) {
    return "";
  }

  return `https://github.com/${owner}/${repo}/actions/runs/${run}`;
}

function actionRunUrl(ref: ActionRunRef): string {
  return `https://github.com/${ref.owner}/${ref.repo}/actions/runs/${ref.runId}`;
}

function writeActionRunParams(ref: ActionRunRef): void {
  const params = new URLSearchParams(window.location.search);
  params.set("owner", ref.owner);
  params.set("repo", ref.repo);
  params.set("run", ref.runId);
  params.delete("url");
  params.delete("actionUrl");
  params.delete("action");
  params.delete("runId");

  const nextUrl = `${window.location.pathname}?${params.toString()}${window.location.hash}`;
  window.history.replaceState(null, "", nextUrl);
}

function CaseDetail({
  bundle,
  item,
  lhs,
  rhs,
}: {
  bundle: LoadedRendererDiffBundle;
  item: RendererDiffCase;
  lhs: string;
  rhs: string;
}) {
  const lhsAsset = item.assets[lhs];
  const rhsAsset = item.assets[rhs];
  const comparison = comparisonFor(item, lhs, rhs);

  return (
    <>
      <div className="case-header">
        <div>
          <h2>{item.name}</h2>
          <span
            className={`status-badge ${statusClass(caseStatusFromComparison(item, comparison))}`}
          >
            {statusLabel(caseStatusFromComparison(item, comparison))}
          </span>
        </div>
        <div className="header-metrics">
          <Metric label="pHash" value={comparison?.metrics.perceptualHashDistance ?? "-"} />
          <Metric
            label="Mismatch"
            value={comparison ? formatPercent(comparison.metrics.pixelMismatchRatio) : "-"}
          />
          <Metric label="MAE" value={comparison?.metrics.meanAbsoluteError.toFixed(5) ?? "-"} />
          <Metric label="Max Delta" value={comparison?.metrics.maxChannelDelta ?? "-"} />
        </div>
      </div>

      <div className="image-grid two-up">
        <ImagePane
          title={groupLabel(bundle, lhs)}
          src={lhsAsset ? bundle.urls.get(lhsAsset.png) : undefined}
          info={lhsAsset}
        />
        <ImagePane
          title={groupLabel(bundle, rhs)}
          src={rhsAsset ? bundle.urls.get(rhsAsset.png) : undefined}
          info={rhsAsset}
        />
      </div>

      <div className="metadata-grid">
        {lhsAsset && (
          <Metadata label={`${groupLabel(bundle, lhs)} hash`} value={lhsAsset.perceptualHash} />
        )}
        {rhsAsset && (
          <Metadata label={`${groupLabel(bundle, rhs)} hash`} value={rhsAsset.perceptualHash} />
        )}
        {lhsAsset && (
          <Metadata label={`${groupLabel(bundle, lhs)} sha256`} value={lhsAsset.sha256Digest} />
        )}
        {rhsAsset && (
          <Metadata label={`${groupLabel(bundle, rhs)} sha256`} value={rhsAsset.sha256Digest} />
        )}
        {comparison && (
          <Metadata
            label="Pixel mismatches"
            value={String(comparison.metrics.pixelMismatchCount)}
          />
        )}
        {lhsAsset && rhsAsset && (
          <Metadata
            label="Dimensions"
            value={`${lhsAsset.width}x${lhsAsset.height} -> ${rhsAsset.width}x${rhsAsset.height}`}
          />
        )}
        {bundle.manifest.source.typstRef && (
          <Metadata label="Typst ref" value={bundle.manifest.source.typstRef} />
        )}
      </div>

      {item.error && <pre className="error-box">{item.error}</pre>}
    </>
  );
}

function ImagePane({
  title,
  src,
  info,
}: {
  title: string;
  src?: string;
  info?: { width: number; height: number };
}) {
  return (
    <figure className="image-pane">
      <figcaption>
        <span>{title}</span>
        {info && (
          <span>
            {info.width} x {info.height}
          </span>
        )}
      </figcaption>
      {src ? <img src={src} alt={title} /> : <div className="missing-image">Missing PNG</div>}
    </figure>
  );
}

function Metric({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function Metadata({ label, value }: { label: string; value: string }) {
  return (
    <div className="metadata">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

async function loadArtifacts(
  ref: ActionRunRef,
  artifacts: GithubArtifact[],
  onProgress: (progress: LoadProgress) => void,
) {
  const bundles: LoadedRendererDiffBundle[] = [];
  const failures: string[] = [];

  for (const [index, artifact] of artifacts.entries()) {
    onProgress({
      current: index,
      total: artifacts.length,
      label: "Loading renderer diff bundles",
      detail: artifact.name,
    });
    try {
      const zip = await downloadArtifactZip(ref, artifact);
      bundles.push(await loadRendererDiffZip(artifact.name, zip));
    } catch (error) {
      failures.push(`${artifact.name}: ${errorMessage(error)}`);
    }
    onProgress({
      current: index + 1,
      total: artifacts.length,
      label: "Loading renderer diff bundles",
      detail: artifact.name,
    });
  }

  return { bundles, failures };
}

function loadSummaryMessage(bundleCount: number, failures: string[]): string {
  if (!failures.length) {
    return `Loaded ${bundleCount} renderer diff bundle(s).`;
  }

  const suffix =
    bundleCount === 0
      ? " Artifact proxy download failed; check that the worker can fetch GitHub Actions artifact ZIPs, or use ZIP upload as fallback."
      : "";
  return `Loaded ${bundleCount} bundle(s). ${failures.join(" ")}${suffix}`;
}

function selectedGroupPair(
  bundle: LoadedRendererDiffBundle | null,
  lhs: string,
  rhs: string,
): [string, string] | null {
  if (!bundle || bundle.manifest.groups.length < 2) {
    return null;
  }

  const groupIds = new Set(bundle.manifest.groups.map((group) => group.id));
  const [defaultLhs, defaultRhs] = defaultGroupPair(bundle);
  const safeLhs = groupIds.has(lhs) ? lhs : defaultLhs;
  const fallbackRhs =
    bundle.manifest.groups.find((group) => group.id !== safeLhs)?.id ?? defaultRhs;
  const safeRhs = groupIds.has(rhs) && rhs !== safeLhs ? rhs : fallbackRhs;
  return [safeLhs, safeRhs];
}

function defaultGroupPair(bundle: LoadedRendererDiffBundle): [string, string] {
  const groups = bundle.manifest.groups;
  const official = groups.find((group) => group.id === "official")?.id ?? groups[0]?.id ?? "";
  const vello =
    groups.find((group) => group.id === "vello")?.id ??
    groups.find((group) => group.id !== official)?.id ??
    "";
  return [official, vello];
}

function groupLabel(bundle: LoadedRendererDiffBundle, id: string): string {
  return bundle.manifest.groups.find((group) => group.id === id)?.label ?? id;
}

function hasGroupAssets(item: RendererDiffCase, lhs: string, rhs: string): boolean {
  return Boolean(item.assets[lhs] && item.assets[rhs]);
}

function comparisonFor(
  item: RendererDiffCase,
  lhs: string,
  rhs: string,
): RendererDiffComparison | undefined {
  return item.comparisons.find(
    (comparison) =>
      (comparison.lhs === lhs && comparison.rhs === rhs) ||
      (comparison.lhs === rhs && comparison.rhs === lhs),
  );
}

function caseStatus(item: RendererDiffCase, lhs: string, rhs: string): string {
  return caseStatusFromComparison(item, comparisonFor(item, lhs, rhs));
}

function caseStatusFromComparison(
  item: RendererDiffCase,
  comparison: RendererDiffComparison | undefined,
): string {
  return comparison?.status ?? item.status;
}

function summarizePair(cases: RendererDiffCase[], lhs: string, rhs: string) {
  const summary = { total: 0, matched: 0, different: 0, renderErrors: 0 };
  for (const item of cases) {
    if (!hasGroupAssets(item, lhs, rhs)) {
      continue;
    }
    summary.total += 1;
    const status = caseStatus(item, lhs, rhs);
    if (status === "matched") {
      summary.matched += 1;
    } else if (status === "render-error") {
      summary.renderErrors += 1;
    } else {
      summary.different += 1;
    }
  }
  return summary;
}

function compareCases(
  lhs: RendererDiffCase,
  rhs: RendererDiffCase,
  lhsGroup: string,
  rhsGroup: string,
): number {
  const lhsComparison = comparisonFor(lhs, lhsGroup, rhsGroup);
  const rhsComparison = comparisonFor(rhs, lhsGroup, rhsGroup);
  const status =
    statusWeight(caseStatusFromComparison(rhs, rhsComparison)) -
    statusWeight(caseStatusFromComparison(lhs, lhsComparison));
  if (status !== 0) {
    return status;
  }

  const lhsMetrics = lhsComparison?.metrics;
  const rhsMetrics = rhsComparison?.metrics;
  const mismatch = (rhsMetrics?.pixelMismatchRatio ?? 0) - (lhsMetrics?.pixelMismatchRatio ?? 0);
  if (mismatch !== 0) {
    return mismatch;
  }

  return (rhsMetrics?.perceptualHashDistance ?? 0) - (lhsMetrics?.perceptualHashDistance ?? 0);
}

function statusWeight(status: string): number {
  if (status === "render-error") {
    return 3;
  }
  if (status === "different") {
    return 2;
  }
  if (status === "matched") {
    return 1;
  }
  return 0;
}

function filterLabel(status: StatusFilter): string {
  switch (status) {
    case "render-error":
      return "Errors";
    case "different":
      return "Diff";
    case "matched":
      return "Matched";
    default:
      return "All";
  }
}

function statusLabel(status: string): string {
  switch (status) {
    case "render-error":
      return "Render error";
    case "different":
      return "Different";
    case "matched":
      return "Matched";
    default:
      return status;
  }
}

function statusClass(status: string): string {
  return status.replace(/[^a-z0-9-]/gi, "-").toLowerCase();
}

function formatPercent(value: number): string {
  return `${(value * 100).toFixed(value < 0.001 ? 3 : 2)}%`;
}

function formatBytes(value: number): string {
  if (value < 1024) {
    return `${value} B`;
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KiB`;
  }
  return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
