import fs from "fs";

const ciFile = fs.readFileSync(".github/workflows/release-vscode.yml", "utf-8");

const checkMinVersion = ciFile.match(
  /# region: check-min-version([\s\S]+?)# end-region: check-min-version/gm,
);
if (!checkMinVersion) {
  console.error("Check minimum version not found");
  process.exit(1);
}

const toolchainRe = /dtolnay\/rust-toolchain@(\d+\.\d+\.\d+)/g;

const ciCheck = checkMinVersion[0];
const ciCheckedVersion = ciCheck.match(toolchainRe)[1];

const cargoToml = fs.readFileSync("Cargo.toml", "utf-8");
const cargoSpecifiedVersion = cargoToml.match(/rust-version = "(\d+\.\d+)"/)[1];

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
if (versionLess(specified, checked)) {
  console.error(
    `Specified version ${cargoSpecifiedVersion} is less than checked version ${ciCheckedVersion}`,
  );
  process.exit(1);
}
