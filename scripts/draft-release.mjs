import { exec } from "child_process";
import fs from "fs";

const removePrefix = (str, prefix) => {
  if (str.startsWith(prefix)) {
    return str.slice(prefix.length);
  }
  return str;
};

const versionToUpload = removePrefix(process.argv[2], "v");

const DIST_CMD = "dist";
// const DIST_CMD = "cargo run --manifest-path ../cargo-dist/cargo-dist/Cargo.toml --bin dist --";

const run = (command) => {
  return new Promise((resolve, reject) => {
    exec(command, (error, stdout, stderr) => {
      if (error) {
        reject(error);
      }
      resolve(stdout);
    });
  });
};

const generateExtensionInstall = (version) => {
  /**
   * @typedef {{ file: string; displayName: string; }} PlatformAsset
   * @typedef {{ name: string; displayName: string; assets: PlatformAsset[]; }} Platform
   */

  /**
   * @param {string} name
   * @returns {string}
   */
  const binExt = (name) => {
    return name.includes("win32") ? ".exe" : "";
  };

  /**
   * @param {string} name
   * @param {string} displayName
   * @returns {Platform}
   */
  const platform = (name, displayName) => ({
    name,
    displayName,
    assets: [{ file: `tinymist-${name}${binExt(name)}`, displayName: "Binary" }],
  });

  /**
   * @type {Platform[]}
   */
  const platforms = [
    platform("win32-x64", "x64 Windows"),
    platform("win32-arm64", "ARM64 Windows"),
    platform("linux-x64", "x64 Linux"),
    platform("linux-arm64", "ARM64 Linux"),
    platform("linux-armhf", "ARMv7 Linux"),
    platform("darwin-x64", "Intel macOS"),
    platform("darwin-arm64", "Apple Silicon macOS"),
    platform("alpine-x64", "x64 Alpine Linux"),
    // Disabled since v0.14.12 because GitHub Actions runners do not reliably support ARM64 Alpine containers.
    // Once GitHub Actions adds stable support for ARM64 Alpine, feel free to submit a PR to re-enable this target.
    // platform("alpine-arm64", "ARM64 Alpine Linux"),
    {
      name: "web",
      displayName: "Browser (Web)",
      assets: [],
    },
    {
      name: "universal",
      displayName: "Other Platforms (Universal)",
      assets: [],
    },
  ];

  const urlBase = `https://github.com/Myriad-Dreamin/tinymist/releases/download/v${version}`;

  const rows = platforms.map((platform) => {
    const file = `[tinymist-${platform.name}.vsix](${urlBase}/tinymist-${platform.name}.vsix)`;
    const assets = platform.assets
      .map((asset) => {
        return `[${asset.displayName}](${urlBase}/${asset.file})`;
      })
      .join(", ");
    return `| ${file} | ${platform.displayName} | ${assets} |`;
  });

  const table = rows.join("\n");

  return `## Download tinymist VS Code Extension ${version}
|  File  | Platform | Assets |
|--------|----------|--------|
${table}
`;
};

const generateGpuViewerInstall = (version) => {
  const platforms = [
    {
      name: "win32-x64",
      displayName: "x64 Windows",
      viewerTarget: "x86_64-pc-windows-msvc",
      viewerArchive: "zip",
    },
    {
      name: "win32-arm64",
      displayName: "ARM64 Windows",
      viewerTarget: "aarch64-pc-windows-msvc",
      viewerArchive: "zip",
    },
    {
      name: "linux-x64",
      displayName: "x64 Linux",
      viewerTarget: "x86_64-unknown-linux-gnu",
      viewerArchive: "tar.gz",
    },
    {
      name: "linux-arm64",
      displayName: "ARM64 Linux",
      viewerTarget: "aarch64-unknown-linux-gnu",
      viewerArchive: "tar.gz",
    },
    {
      name: "linux-armhf",
      displayName: "ARMv7 Linux",
      viewerTarget: "arm-unknown-linux-gnueabihf",
      viewerArchive: "tar.gz",
    },
    {
      name: "darwin-x64",
      displayName: "Intel macOS",
      viewerTarget: "x86_64-apple-darwin",
      viewerArchive: "tar.gz",
    },
    {
      name: "darwin-arm64",
      displayName: "Apple Silicon macOS",
      viewerTarget: "aarch64-apple-darwin",
      viewerArchive: "tar.gz",
    },
  ];

  const urlBase = `https://github.com/Myriad-Dreamin/tinymist/releases/download/v${version}`;
  const rows = platforms.map((platform) => {
    const extension = `tinymist-gpu-viewer-${platform.name}.vsix`;
    const viewer = `tinymist-viewer-${platform.viewerTarget}.${platform.viewerArchive}`;
    return `| [${extension}](${urlBase}/${extension}) | ${platform.displayName} | [${viewer}](${urlBase}/${viewer}) |`;
  });

  return `## Download tinymist-gpu-viewer VS Code Extension ${version}
|  Extension  | Platform | Native Viewer |
|-------------|----------|----------------|
${rows.join("\n")}
`;
};

const generateIntellijPluginInstall = (version) => {
  const gradleProperties = fs.readFileSync("editors/intellij/gradle.properties", "utf8");
  const pluginVersion = gradleProperties.match(/^pluginVersion\s*=\s*(.+)$/m)?.[1].trim();
  if (!pluginVersion) {
    throw new Error(
      "Failed to find the IntelliJ plugin version in editors/intellij/gradle.properties",
    );
  }

  const file = `tinymist-intellij-${pluginVersion}.zip`;
  const url = `https://github.com/Myriad-Dreamin/tinymist/releases/download/v${version}/${file}`;
  return `## Download Tinymist IntelliJ Plugin ${pluginVersion}
[${file}](${url})
`;
};

const collapsed = (content, summary) => {
  return `<details>

<summary><strong>${summary}</strong></summary>

${content}

</details>`;
};

const main = async () => {
  if (!versionToUpload) {
    console.error("Please provide the version to upload");
    process.exit(1);
  }

  if (process.env.GITHUB_OUTPUT) {
    const output = `tag=v${versionToUpload}`;
    fs.appendFileSync(process.env.GITHUB_OUTPUT, output + "\n");
  }

  // read version from packages.json
  const packageJson = JSON.parse(fs.readFileSync("./editors/vscode/package.json", "utf8"));
  if (packageJson.version !== versionToUpload) {
    console.error(
      `Version in Cargo.toml (${packageJson.version}) is different from the version to upload (${versionToUpload})`,
    );
    process.exit(1);
  }

  // run dist host command
  // remove target/distrib/dist-manifest.json which causes stateful announce...
  if (fs.existsSync("target/distrib/dist-manifest.json")) {
    fs.unlinkSync("target/distrib/dist-manifest.json");
  }

  await run(DIST_CMD + " generate");

  const distManifest = await run(
    DIST_CMD + " host --steps=upload --steps=release --output-format=json",
  );
  const distData = JSON.parse(distManifest);
  const binInstallText = distData.announcement_github_body;
  // write to file
  fs.writeFileSync("target/announcement-dist.md", binInstallText);

  // parse-changelog .\editors\vscode\CHANGELOG.md
  const changelogPlainRaw = await run("parse-changelog ./editors/vscode/CHANGELOG.md");
  // **Full Changelog**:
  // Patch the full changelog link
  const fullChangelogLine =
    /\*\*Full Changelog\*\*: https:\/\/github.com\/Myriad-Dreamin\/tinymist\/compare\/v(\d+\.\d+\.\d+)...v(\d+\.\d+\.\d+)/;
  let anyMatched = false;
  const changelogPlain = changelogPlainRaw.replace(fullChangelogLine, (_match, p1, p2) => {
    anyMatched = true;
    if (!versionToUpload.startsWith(p2)) {
      console.error(
        `Failed to patch the full changelog link, expected version to upload to start with ${p2}, but got ${versionToUpload}`,
      );
      process.exit(1);
    }

    return `\*\*Full Changelog\*\*: https://github.com/Myriad-Dreamin/tinymist/compare/v${p1}...v${versionToUpload}`;
  });
  if (!anyMatched) {
    console.error("Failed to patch the full changelog link");
    process.exit(1);
  }

  fs.writeFileSync("target/announcement-changelog.md", changelogPlain);

  const extensionInstallText = [
    generateExtensionInstall(versionToUpload),
    generateGpuViewerInstall(versionToUpload),
  ].join("\n\n");
  const intellijPluginInstallText = generateIntellijPluginInstall(versionToUpload);
  // concat and generate final announcement
  const binInstallSection = collapsed(binInstallText, `Download Binary`);
  const extensionInstallSection = collapsed(extensionInstallText, `Download VS Code Extensions`);
  const intellijPluginInstallSection = collapsed(
    intellijPluginInstallText,
    `Download IntelliJ Plugin`,
  );
  const announcement = [
    changelogPlain,
    binInstallSection,
    extensionInstallSection,
    intellijPluginInstallSection,
  ].join("\n\n");
  fs.writeFileSync("target/announcement.gen.md", announcement);

  console.log("Please check the generated announcement in target/announcement.gen.md");
};

main();
