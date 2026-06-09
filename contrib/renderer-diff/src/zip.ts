import JSZip from "jszip";
import type { LoadedRendererDiffBundle, RendererDiffManifest } from "./types";
import { rendererDiffManifestFile } from "./types";

export async function loadRendererDiffZip(
  artifactName: string,
  data: ArrayBuffer | Blob,
): Promise<LoadedRendererDiffBundle> {
  const zip = await JSZip.loadAsync(data);
  const entries = Object.values(zip.files).filter((entry) => !entry.dir);
  const manifestEntry =
    entries.find((entry) => entry.name === rendererDiffManifestFile) ??
    entries.find((entry) => entry.name.endsWith(`/${rendererDiffManifestFile}`));

  if (!manifestEntry) {
    throw new Error(`${artifactName} does not contain ${rendererDiffManifestFile}`);
  }

  const basePath = manifestEntry.name.slice(
    0,
    manifestEntry.name.length - rendererDiffManifestFile.length,
  );
  const manifest = JSON.parse(await manifestEntry.async("string")) as RendererDiffManifest;
  validateManifest(manifest, artifactName);

  const urls = new Map<string, string>();
  for (const item of manifest.cases) {
    for (const asset of Object.values(item.assets)) {
      await addObjectUrl(zip, basePath, asset.png, urls);
      requireProtocolAsset(zip, basePath, asset.hash);
      requireProtocolAsset(zip, basePath, asset.sha256);
    }
  }

  return {
    artifactName: artifactName || manifest.artifactName,
    manifest,
    urls,
    entryNames: entries.map((entry) => entry.name),
  };
}

export function revokeRendererDiffBundle(bundle: LoadedRendererDiffBundle): void {
  for (const url of bundle.urls.values()) {
    URL.revokeObjectURL(url);
  }
}

async function addObjectUrl(
  zip: JSZip,
  basePath: string,
  protocolPath: string,
  urls: Map<string, string>,
  required = true,
): Promise<void> {
  const entry = zip.file(`${basePath}${protocolPath}`) ?? zip.file(protocolPath);
  if (!entry) {
    if (required) {
      throw new Error(`missing protocol asset ${protocolPath}`);
    }
    return;
  }

  const blob = await entry.async("blob");
  urls.set(protocolPath, URL.createObjectURL(blob));
}

function validateManifest(manifest: RendererDiffManifest, artifactName: string): void {
  if (manifest.schemaVersion !== 1) {
    throw new Error(`${artifactName} uses unsupported schema version ${manifest.schemaVersion}`);
  }
  if (!Array.isArray(manifest.cases)) {
    throw new Error(`${artifactName} manifest has no cases array`);
  }
  if (!manifest.summary || typeof manifest.summary.total !== "number") {
    throw new Error(`${artifactName} manifest has no summary`);
  }
  if (!Array.isArray(manifest.groups) || manifest.groups.length < 2) {
    throw new Error(`${artifactName} manifest must define at least two groups`);
  }
}

function requireProtocolAsset(zip: JSZip, basePath: string, protocolPath: string): void {
  const entry = zip.file(`${basePath}${protocolPath}`) ?? zip.file(protocolPath);
  if (!entry) {
    throw new Error(`missing protocol asset ${protocolPath}`);
  }
}
