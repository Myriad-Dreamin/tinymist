export const rendererDiffManifestFile = "renderer-diff-manifest.json";
export const rendererDiffArtifactPrefix = "renderer-diff-";

export interface RendererDiffManifest {
  schemaVersion: number;
  artifactName: string;
  groups: RendererDiffGroup[];
  source: RendererDiffSource;
  hash: RendererDiffHashInfo;
  summary: RendererDiffSummary;
  cases: RendererDiffCase[];
}

export interface RendererDiffGroup {
  id: string;
  label: string;
  kind: string;
  source?: string;
}

export interface RendererDiffSource {
  suite: string;
  typstTests: string;
  typstRef?: string;
  githubRunId?: string;
  githubSha?: string;
}

export interface RendererDiffHashInfo {
  algorithm: string;
  bits: number;
  format: string;
  distance: string;
}

export interface RendererDiffSummary {
  total: number;
  matched: number;
  different: number;
  renderErrors: number;
}

export interface RendererDiffCase {
  name: string;
  status: RendererDiffStatus;
  assets: Record<string, RendererDiffAsset>;
  comparisons: RendererDiffComparison[];
  error?: string;
}

export type RendererDiffStatus = "matched" | "different" | "render-error" | string;

export interface RendererDiffAsset {
  png: string;
  hash: string;
  sha256: string;
  width: number;
  height: number;
  perceptualHash: string;
  sha256Digest: string;
}

export interface RendererDiffComparison {
  lhs: string;
  rhs: string;
  status: RendererDiffStatus;
  metrics: RendererDiffMetrics;
}

export interface RendererDiffMetrics {
  perceptualHashDistance: number;
  pixelMismatchCount: number;
  pixelMismatchRatio: number;
  meanAbsoluteError: number;
  maxChannelDelta: number;
}

export interface LoadedRendererDiffBundle {
  artifactName: string;
  manifest: RendererDiffManifest;
  urls: Map<string, string>;
  entryNames: string[];
}
