/**
 * Check if the current environment is Gitpod.
 * @return True if the current environment is Gitpod, false otherwise.
 */
export function isGitpod(): boolean {
  return !!process.env.GITPOD_WORKSPACE_ID && !!process.env.GITPOD_WORKSPACE_CLUSTER_HOST;
}

/**
 * Create a Gitpod URL for the given URL string.
 * @param urlStr The URL string to create a Gitpod URL for.
 * @return The Gitpod URL
 */
export function translateGitpodURL(urlStr: string): string {
  const url = new URL(urlStr);
  if (!url.port) {
    throw new Error("port is not specified in the URL");
  }
  if (!isGitpod()) {
    throw new Error("not in Gitpod environment");
  }
  if (url.protocol.match("ws")) {
    url.protocol = "wss";
  }
  url.hostname =
    url.port.toString() +
    "-" +
    process.env.GITPOD_WORKSPACE_ID +
    "." +
    process.env.GITPOD_WORKSPACE_CLUSTER_HOST;
  url.port = "";
  return url.toString();
}
