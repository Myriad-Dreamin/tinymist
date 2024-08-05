import { LanguageClient } from "vscode-languageclient/node";
import { spawnSync } from "child_process";
import { resolve } from "path";

export let client: LanguageClient | undefined = undefined;

export function setClient(newClient: LanguageClient) {
  client = newClient;
  clientPromiseResolve(newClient);
}

let clientPromiseResolve = (_client: LanguageClient) => {};
let clientPromise: Promise<LanguageClient> = new Promise((resolve) => {
  clientPromiseResolve = resolve;
});
export async function getClient(): Promise<LanguageClient> {
  return clientPromise;
}

interface ResourceRoutes {
  "/symbols": any;
  "/preview/index.html": string;
}

export const tinymist = {
  probeEnvPath,
  probePaths,
  async executeCommand<R>(command: string, args: any[]) {
    return await (
      await getClient()
    ).sendRequest<R>("workspace/executeCommand", {
      command,
      arguments: args,
    });
  },
  getResource<T extends keyof ResourceRoutes>(path: T) {
    return tinymist.executeCommand<ResourceRoutes[T]>("tinymist.getResources", [path]);
  },
  exportPdf(uri: string, extraOpts?: any) {
    return doExport("tinymist.exportPdf", uri, extraOpts);
  },
  exportSvg(uri: string, extraOpts?: any) {
    return doExport("tinymist.exportSvg", uri, extraOpts);
  },
  exportPng(uri: string, extraOpts?: any) {
    return doExport("tinymist.exportPng", uri, extraOpts);
  },
  exportHtml(uri: string, extraOpts?: any) {
    return doExport("tinymist.exportHtml", uri, extraOpts);
  },
  exportMarkdown(uri: string, extraOpts?: any) {
    return doExport("tinymist.exportMarkdown", uri, extraOpts);
  },
  exportText(uri: string, extraOpts?: any) {
    return doExport("tinymist.exportText", uri, extraOpts);
  },
  exportQuery(uri: string, extraOpts?: any) {
    return doExport("tinymist.exportQuery", uri, extraOpts);
  },
  showLog() {
    if (client) {
      client.outputChannel.show();
    }
  },
};

/// kill the probe task after 60s
const PROBE_TIMEOUT = 60_000;

function probeEnvPath(configName: string, configPath?: string): string {
  const isWindows = process.platform === "win32";
  const binarySuffix = isWindows ? ".exe" : "";
  const binaryName = "tinymist" + binarySuffix;

  const serverPaths: [string, string][] = configPath
    ? [[`\`${configName}\` (${configPath})`, configPath as string]]
    : [
        ["Bundled", resolve(__dirname, binaryName)],
        ["In PATH", binaryName],
      ];

  return tinymist.probePaths(serverPaths);
}

function probePaths(paths: [string, string][]): string {
  const messages = [];
  for (const [loc, path] of paths) {
    let messageSuffix;
    try {
      console.log("validate", path, "args", ["probe"]);
      const result = spawnSync(path, ["probe"], { timeout: PROBE_TIMEOUT });
      if (result.status === 0) {
        return path;
      }

      const statusMessage = result.status !== null ? [`return status: ${result.status}`] : [];
      const errorMessage =
        result.error?.message !== undefined ? [`error: ${result.error.message}`] : [];
      const messages = [statusMessage, errorMessage];
      messageSuffix = messages.length !== 0 ? `:\n\t${messages.flat().join("\n\t")}` : "";
    } catch (e) {
      if (e instanceof Error) {
        messageSuffix = `: ${e.message}`;
      } else {
        messageSuffix = `: ${JSON.stringify(e)}`;
      }
    }

    messages.push([loc, path, `failed to probe${messageSuffix}`]);
  }

  const infos = messages.map(([loc, path, message]) => `${loc} ('${path}'): ${message}`).join("\n");
  throw new Error(`Could not find a valid tinymist binary.\n${infos}`);
}

function doExport(command: string, uri: string, extraOpts?: any): Promise<string> {
  return tinymist.executeCommand<string>(command, [uri, ...(extraOpts ? [extraOpts] : [])]);
}
