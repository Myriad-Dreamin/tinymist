
import { readFileSync } from 'fs';


function check() {
    const cargoToml = readFileSync('../../Cargo.toml', 'utf8');
    const cargoVersion = cargoToml.match(/version = "(.*?)"/)[1];
    const pkgVersion = JSON.parse(readFileSync('package.json', 'utf8')).version;

    if (cargoVersion !== pkgVersion) {
        throw new Error(`Version mismatch: ${cargoVersion} (in Cargo.toml) !== ${pkgVersion} (in package.json)`);
    }
}

check();
