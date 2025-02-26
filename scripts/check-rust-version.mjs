import fs from "fs";

const ciFile = fs.readFileSync(".github/workflows/release-vscode.yml", "utf-8");

const toolchainRe = (forWhat) =>
  new RegExp(/dtolnay\/rust-toolchain@(\d+\.\d+\.\d+)/.source + `\\s*#\\s*${forWhat}`);

const ciCheckedVersion = ciFile.match(toolchainRe("check-min-version"))?.[1];
if (!ciCheckedVersion) {
  console.error("Check minimum version not found");
  process.exit(1);
}

const ciBuildVersion = ciFile.match(toolchainRe("build-extension-version"))?.[1];
if (!ciBuildVersion) {
  console.error("Build extension version not found");
  process.exit(1);
}

const cargoToml = fs.readFileSync("Cargo.toml", "utf-8");
const cargoSpecifiedVersion = cargoToml.match(/rust-version = "(\d+\.\d+)"/)[1];

const toolchainToml = fs.readFileSync("rust-toolchain.toml", "utf-8");
const toolchainSpecifiedVersion = toolchainToml.match(/channel = "(\d+\.\d+)"/)[1];

function parseVersion(version) {
  const versions = version.split(".").map(Number);
  if (versions.length === 2) {
    versions.push(0);
  }
  if (versions.length !== 3) {
    throw new Error(`Invalid version: ${version}`);
  }
  return versions;
}

function versionLess(a, b) {
  if (a.length !== 3 || b.length !== 3) {
    throw new Error("Invalid version to compare");
  }

  return (
    a[0] < b[0] || (a[0] === b[0] && a[1] < b[1]) || (a[0] === b[0] && a[1] === b[1] && a[2] < b[2])
  );
}

const specified = parseVersion(cargoSpecifiedVersion);
const checked = parseVersion(ciCheckedVersion);
const build = parseVersion(ciBuildVersion);
const toolchain = parseVersion(toolchainSpecifiedVersion);

if (versionLess(specified, checked)) {
  console.error(
    `Specified version ${cargoSpecifiedVersion} is less than checked version ${ciCheckedVersion}`,
  );
  process.exit(1);
}

if (versionLess(build, toolchain) || versionLess(toolchain, build)) {
  console.error(
    `Build version ${ciBuildVersion} does not match toolchain version ${toolchainSpecifiedVersion}`,
  );
  process.exit(1);
}

if (versionLess(toolchain, checked)) {
  console.error(
    `Toolchain version ${toolchainSpecifiedVersion} is less than checked version ${ciCheckedVersion}`,
  );
  process.exit(1);
}
