import { rendererDiffArtifactPrefix } from "./types";

const artifactProxyUrl = "https://github-artifacts.camiyoru.workers.dev/";

export interface ActionRunRef {
  owner: string;
  repo: string;
  runId: string;
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

  return {
    owner: parts[0],
    repo: parts[1],
    runId: parts[4],
  };
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
      throw new Error(`GitHub artifacts request failed with HTTP ${response.status}`);
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

function githubHeaders(): HeadersInit {
  return {
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
  };
}
