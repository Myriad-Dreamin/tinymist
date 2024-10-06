
import { execSync } from 'child_process';
import { resolve } from 'path';
import { readFileSync } from 'fs';

const tinymistExecutable = resolve('out/tinymist' + (process.platform === 'win32' ? '.exe' : ''));

function check() {
    const version = JSON.parse(readFileSync('package.json', 'utf8')).version;
    const expected = `tinymist ${version}`;

    const output = execSync(`${tinymistExecutable} -V`).toString().trim();

    if (output !== expected) {
        throw new Error(`Version mismatch: ${output} !== ${expected} (in package.json)`);
    }
}

check();
