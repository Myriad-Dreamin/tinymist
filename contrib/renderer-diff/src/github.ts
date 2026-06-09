import { rendererDiffArtifactPrefix } from "./types";

const artifactProxyUrl = "https://github-artifacts.camiyoru.workers.dev/";

export interface ActionRunRef {
  owner: string;
  repo: string;
  runId: string;
  artifactId?: string;
}

export interface GithubArtifact {
  id: number;
  name: string;
  expired: boolean;
  size_in_bytes: number;
}

interface GithubArtifactsResponse {
  total_count: number;
  artifacts: GithubArtifact[];
}

export function parseActionRunUrl(input: string): ActionRunRef | null {
  let url: URL;
  try {
    url = new URL(input.trim());
  } catch {
    return null;
  }

  if (url.hostname !== "github.com") {
    return null;
  }

  const parts = url.pathname.split("/").filter(Boolean);
  const actionsIndex = parts.indexOf("actions");
  if (actionsIndex !== 2 || parts[3] !== "runs" || !parts[4]) {
    return null;
  }
  const runId = normalizeActionRunId(parts[4]);
  if (!runId) {
    return null;
  }
  const artifactId = parseActionArtifactId(parts);
  if (artifactId === null) {
    return null;
  }

  return {
    owner: parts[0],
    repo: parts[1],
    runId,
    artifactId,
  };
}

export function normalizeActionRunId(input: string): string | null {
  const raw = input.trim();
  let decoded = raw;
  try {
    decoded = decodeURIComponent(raw);
  } catch {
    decoded = raw;
  }
  const match = decoded.match(/^(\d+)(?:\s.*)?$/);
  return match?.[1] ?? null;
}

export function normalizeActionArtifactId(input: string): string | null {
  return normalizeActionRunId(input);
}

export function githubArtifactFromId(artifactId: string): GithubArtifact {
  return {
    id: Number(artifactId),
    name: `GitHub artifact ${artifactId}`,
    expired: false,
    size_in_bytes: 0,
  };
}

function parseActionArtifactId(parts: string[]): string | null | undefined {
  if (parts.length === 5) {
    return undefined;
  }
  if (parts.length !== 7 || parts[5] !== "artifacts") {
    return null;
  }

  return normalizeActionArtifactId(parts[6]);
}

export async function listRendererDiffArtifacts(
  ref: ActionRunRef,
  signal?: AbortSignal,
): Promise<GithubArtifact[]> {
  const artifacts: GithubArtifact[] = [];
  let page = 1;

  while (true) {
    const url = new URL(
      `https://api.github.com/repos/${ref.owner}/${ref.repo}/actions/runs/${ref.runId}/artifacts`,
    );
    url.searchParams.set("per_page", "100");
    url.searchParams.set("page", String(page));

    const response = await fetch(url, {
      headers: githubHeaders(),
      signal,
    });

    if (!response.ok) {
      throw await githubArtifactsRequestError(response);
    }

    const data = (await response.json()) as GithubArtifactsResponse;
    artifacts.push(
      ...data.artifacts.filter((artifact) => artifact.name.startsWith(rendererDiffArtifactPrefix)),
    );

    if (page * 100 >= data.total_count || data.artifacts.length < 100) {
      break;
    }
    page += 1;
  }

  return artifacts;
}

export async function downloadArtifactZip(
  ref: ActionRunRef,
  artifact: GithubArtifact,
  signal?: AbortSignal,
): Promise<ArrayBuffer> {
  const response = await fetch(artifactProxyDownloadUrl(ref, artifact), {
    signal,
  });

  if (!response.ok) {
    throw new Error(`${artifact.name} artifact proxy failed with HTTP ${response.status}`);
  }

  const data = await response.arrayBuffer();
  if (!isZipArchive(data)) {
    throw new Error(`${artifact.name} artifact proxy returned ${describeNonZipResponse(data)}`);
  }

  return data;
}

export function artifactProxyDownloadUrl(ref: ActionRunRef, artifact: GithubArtifact): string {
  const url = new URL(artifactProxyUrl);
  url.searchParams.set("user", ref.owner);
  url.searchParams.set("repo", ref.repo);
  url.searchParams.set("artifactId", String(artifact.id));
  return url.toString();
}

function isZipArchive(data: ArrayBuffer): boolean {
  if (data.byteLength < 4) {
    return false;
  }

  const bytes = new Uint8Array(data, 0, 4);
  return bytes[0] === 0x50 && bytes[1] === 0x4b;
}

function describeNonZipResponse(data: ArrayBuffer): string {
  if (data.byteLength === 0) {
    return "an empty response";
  }

  const prefix = new TextDecoder().decode(data.slice(0, Math.min(data.byteLength, 32))).trimStart();
  if (prefix.startsWith("<")) {
    return "HTML instead of a ZIP";
  }
  if (prefix.startsWith("{")) {
    return "JSON instead of a ZIP";
  }

  return "non-ZIP bytes";
}

async function githubArtifactsRequestError(response: Response): Promise<Error> {
  const details = [githubArtifactsStatusMessage(response)];
  const apiMessage = await readGithubApiMessage(response);
  if (apiMessage) {
    details.push(apiMessage);
  }

  return new Error(details.join(" "));
}

function githubArtifactsStatusMessage(response: Response): string {
  const reset = githubRateLimitReset(response.headers);
  if (response.status === 403 && response.headers.get("X-RateLimit-Remaining") === "0") {
    return `GitHub API rate limit was exhausted while listing artifacts. Try again after ${reset ?? "the reset time shown by GitHub"}, or use ZIP upload.`;
  }

  if (response.status === 403) {
    return "GitHub refused the artifacts request with HTTP 403. Check that the run URL is correct and public, or use ZIP upload.";
  }

  if (response.status === 404) {
    return "GitHub could not find that Actions run. Check the owner, repository, and run id, then try again.";
  }

  return `GitHub artifacts request failed with HTTP ${response.status}.`;
}

async function readGithubApiMessage(response: Response): Promise<string | null> {
  try {
    const body = (await response.json()) as { message?: unknown };
    return typeof body.message === "string" ? body.message : null;
  } catch {
    return null;
  }
}

function githubRateLimitReset(headers: Headers): string | null {
  const reset = Number(headers.get("X-RateLimit-Reset"));
  if (!Number.isFinite(reset) || reset <= 0) {
    return null;
  }

  return new Date(reset * 1000).toLocaleString();
}

function githubHeaders(): HeadersInit {
  return {
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
  };
}
