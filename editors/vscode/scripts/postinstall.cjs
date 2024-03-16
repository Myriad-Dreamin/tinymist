
const path = require('path');
const fs = require('fs');
const rimraf = require('rimraf');

const vscodeDir = path.join(__dirname, '../');
const editorToolsDir = path.join(vscodeDir, '../../tools/editor-tools/');

rimraf.sync(path.join(vscodeDir, 'out/editor-tools/'));
fs.mkdirSync(path.join(vscodeDir, 'out/editor-tools/'), { recursive: true });

function copyDir(src, dest) {
  fs.readdirSync(src).forEach((item) => {
    const srcPath = path.join(src, item);
    const destPath = path.join(dest, item);
    if (fs.lstatSync(srcPath).isDirectory()) {
        fs.mkdirSync(destPath,
        { recursive: true });
        copyDir(srcPath, destPath);
    }
    else {
        fs.copyFileSync(srcPath, destPath);
    }
});
}

copyDir(path.join(editorToolsDir, "dist"), path.join(vscodeDir, 'out/editor-tools/'));

