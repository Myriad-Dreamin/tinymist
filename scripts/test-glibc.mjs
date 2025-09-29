import { exec as execSync } from "child_process";
import util from "node:util";

const exec = util.promisify(execSync);
const binaryPath = process.argv[2];

// get glibc symbols by l
const symbols = await exec(`objdump -T ${binaryPath}`);

const GLIBC_VERSION_REGEX = /\(GLIBC_([\d.]+)\)/;

const versionSet = new Set(
  symbols.stdout
    .split("\n")
    .filter((line) => line.match(GLIBC_VERSION_REGEX))
    .map((line) => line.match(GLIBC_VERSION_REGEX)[1]),
);

class GLIBCVersion {
  constructor(version) {
    this.semver = version.split(".").map((it) => Number.parseInt(it));
  }
}

const versions = Array.from(versionSet)
  .map((version) => new GLIBCVersion(version))
  .filter((it) => it.semver.length == 2);

let maxMinor = 0;
for (const version of versions) {
  if (version.semver[0] !== 2) {
    throw new Error("GLIBC version is not 2");
  }
  if (version.semver[1] > maxMinor) {
    maxMinor = version.semver[1];
  }
}

console.log(maxMinor);

if (maxMinor >= 35) {
  throw new Error(`GLIBC version is greater than 2.35, got GLIBC_2.${maxMinor}`);
}
