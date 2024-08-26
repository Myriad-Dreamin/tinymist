import { window, workspace } from 'vscode';
import { tinymist } from './lsp';
import * as fs from 'fs';

// error message
export const dataDirErrorMessage = 'Can not find package directory.';

export async function getLocalPackagesDir() {
    const packagesDir = await tinymist.getResource('/dirs/local-packages');
    return packagesDir ? `${packagesDir}/local` : null;
}

// typst.toml template
const typstTomlTemplate = (name: string, version: string, entrypoint: string) => {
    return `[package]\nname = "${name}"\nversion = "${version}"\nentrypoint = "${entrypoint}"`;
};

// versionCompare
function versionCompare(a: string, b: string) {
    const aArr = a.split('.');
    const bArr = b.split('.');
    for (let i = 0; i < 3; i++) {
        const aNum = Number(aArr[i]);
        const bNum = Number(bArr[i]);
        if (aNum !== bNum) {
            return bNum - aNum;
        }
    }
    return 0;
}

/**
 * get local packages list
 */
export async function getLocalPackagesList() {
    const localPackagesDir = await getLocalPackagesDir();
    // return list of local packages like ['@local/mypkg:1.0.0']
    if (!localPackagesDir) {
        return [];
    }
    // if localPackagesDir doesn't exist, return []
    try {
        await fs.promises.access(localPackagesDir);
    } catch (err) {
        return [];
    }
    const localPackagesList = await fs.promises.readdir(localPackagesDir);
    // get all version
    const res = [] as {
        package: string,
        namespace: string,
        name: string,
        version: string,
    }[];
    for (const localPackage of localPackagesList) {
        // if localPackage is not a directory, continue
        const stat = await fs.promises.stat(`${localPackagesDir}/${localPackage}`);
        if (!stat.isDirectory()) {
            continue;
        }
        // filter versions only valid version like '0.1.0'
        const versions = (await fs.promises.readdir(`${localPackagesDir}/${localPackage}`)).filter(version => {
            const versionReg = /^\d+\.\d+\.\d+$/;
            return versionReg.test(version);
        });
        // sort versions like ['1.0.0', '0.2.0', '0.1.0', '0.0.2', '0.0.1']
        versions.sort(versionCompare);
        for (const version of versions) {
            res.push({
                package: `@local/${localPackage}:${version}`,
                namespace: 'local',
                name: localPackage,
                version,
            });
        }
    }
    return res;
}

/**
 * create local package
 */
export async function commandCreateLocalPackage() {
    const localPackagesDir = await getLocalPackagesDir();
    if (!localPackagesDir) {
        window.showErrorMessage(dataDirErrorMessage);
        return;
    }
    // 1. input package name
    const packageName = await window.showInputBox({
        value: '',
        placeHolder: 'Please input package name',
        validateInput: text => {
            return text ? null : 'Please input package name';
        }
    });
    if (!packageName) {
        return;
    }
    // 2. input package version
    const packageVersion = await window.showInputBox({
        value: '0.1.0',
        placeHolder: 'Please input package version',
        validateInput: text => {
            if (!text) {
                return 'Please input package version';
            }
            // make sure it is valid version like '0.1.0'
            const versionReg = /^\d+\.\d+\.\d+$/;
            if (!versionReg.test(text)) {
                return 'Please input valid package version like 0.1.0';
            }
            return null;
        }
    });
    if (!packageVersion) {
        return;
    }
    // 3. input entrypoint
    const entrypoint = await window.showInputBox({
        value: 'lib.typ',
        placeHolder: 'Please input entrypoint',
        validateInput: text => {
            if (!text) {
                return 'Please input entrypoint';
            }
            // make sure it is valid entrypoint end with .typ
            if (!text.endsWith('.typ')) {
                return 'Please input valid entrypoint end with .typ';
            }
            return null;
        }
    });
    if (!entrypoint) {
        return;
    }
    // 4. create localPackagesDir/name/version/typst.toml
    const packageDir = `${localPackagesDir}/${packageName}/${packageVersion}`;
    const typstToml = typstTomlTemplate(packageName, packageVersion, entrypoint);
    await fs.promises.mkdir(packageDir, { recursive: true });
    await fs.promises.writeFile(`${packageDir}/typst.toml`, typstToml);
    // 5. create localPackagesDir/name/version/entrypoint
    await fs.promises.writeFile(`${packageDir}/${entrypoint}`, '#let add(a, b) = { a + b }');
    // 6. open localPackagesDir/name/version/entrypoint
    const document = await workspace.openTextDocument(`${packageDir}/${entrypoint}`);
    await window.showTextDocument(document);
}

/**
 * open local package in editor
 */
export async function commandOpenLocalPackage() {
    const localPackagesDir = await getLocalPackagesDir();
    if (!localPackagesDir) {
        window.showErrorMessage(dataDirErrorMessage);
        return;
    }
    // 1. select local package
    const localPackagesList = await getLocalPackagesList();
    const localPackages = localPackagesList.map(pkg => pkg.package);
    const selected = await window.showQuickPick(localPackages, {
        placeHolder: 'Please select a local package to open'
    });
    if (!selected) {
        return;
    }
    // 2. read localPackagesDir/name/version/typst.toml
    const name = localPackagesList.filter(pkg => pkg.package === selected)[0].name;
    const version = localPackagesList.filter(pkg => pkg.package === selected)[0].version;
    const packageDir = `${localPackagesDir}/${name}/${version}`;
    // if typst.toml doesn't exist, return
    try {
        await fs.promises.access(`${packageDir}/typst.toml`);
    } catch (err) {
        window.showErrorMessage('Can not find typst.toml.');
        return;
    }
    const typstToml = await fs.readFileSync(`${packageDir}/typst.toml`, 'utf-8');
    // parse typst.toml
    const entrypoint = typstToml.match(/entrypoint\s*=\s*"(.*)"/)?.[1];
    if (!entrypoint) {
        // open typst.toml if entrypoint is not set
        const document = await workspace.openTextDocument(`${packageDir}/typst.toml`);
        await window.showTextDocument(document);
        return;
    }
    // 3. open localPackagesDir/name/version/entrypoint
    const document = await workspace.openTextDocument(`${packageDir}/${entrypoint}`);
    await window.showTextDocument(document);
}