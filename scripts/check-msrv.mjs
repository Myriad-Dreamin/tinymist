import fs from 'fs';

const ciFile = fs.readFileSync('.github/workflows/release-vscode.yml', 'utf-8');

const checkMinVersion = ciFile.match(/# region: check-min-version([\s\S]+?)# end-region: check-min-version/mg);
if (!checkMinVersion) {
    console.error('Check minimum version not found');
    process.exit(1);
}

const ciCheck = checkMinVersion[0];
// dtolnay/rust-toolchain@1.83.0
const ciCheckedVersion = ciCheck.match(/dtolnay\/rust-toolchain@(\d+\.\d+\.\d+)/)[1];

const cargoToml = fs.readFileSync('Cargo.toml', 'utf-8');
// rust-version = "1.82"
const cargoSpecifiedVersion = cargoToml.match(/rust-version = "(\d+\.\d+)"/)[1];

function parseVersion(version) {
    const versions = version.split('.').map(Number);
    if (versions.length === 2) {
        versions.push(0);
    }
    if (versions.length !== 3) {
        throw new Error(`Invalid version: ${version}`);
    }
    return versions;
}

const specified = parseVersion(cargoSpecifiedVersion);
const checked = parseVersion(ciCheckedVersion);
if (
    specified[0] < checked[0] ||
    (specified[0] === checked[0] && specified[1] < checked[1]) ||
    (specified[0] === checked[0] && specified[1] === checked[1] && specified[2] < checked[2])
) {
    console.error(`Specified version ${cargoSpecifiedVersion} is less than checked version ${ciCheckedVersion}`);
    process.exit(1);
}
