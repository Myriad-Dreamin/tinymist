import { exec } from 'child_process';
import fs from 'fs';

const versionToUpload = process.argv[2];

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
        return (name.includes('win32') ? '.exe' : '');
    }

    /**
     * @param {string} name 
     * @returns {string}
     */
    const debugExt = (name) => {
        return (name.includes('darwin') ? '.dwarf' : (name.includes('win32') ? '.pdb' : '.debug'));
    }

    /**
     * @param {string} name
     * @param {string} displayName
     * @returns {Platform}
     */
    const platform = (name, displayName) => ({
        name,
        displayName,
        assets: [
            { file: `tinymist-${name}${binExt(name)}`, displayName: 'Binary' },
            { file: `tinymist-${name}${debugExt(name)}`, displayName: 'Debug Symbols' }
        ]
    })

    /**
     * @type {Platform[]}
     */
    const platforms = [
        platform('win32-x64', 'x64 Windows'),
        platform('win32-arm64', 'ARM64 Windows'),
        platform('linux-x64', 'x64 Linux'),
        platform('linux-arm64', 'ARM64 Linux'),
        platform('linux-armhf', 'ARMv7 Linux'),
        platform('darwin-x64', 'Intel macOS'),
        platform('darwin-arm64', 'Apple Silicon macOS'),
        platform('alpine-x64', 'x64 Alpine Linux'),
        platform('alpine-arm64', 'ARM64 Alpine Linux'),
        {
            name: 'universal',
            displayName: 'Other Platforms (Universal)',
            assets: []
        }
    ];

    const urlBase = `https://github.com/Myriad-Dreamin/tinymist/releases/download/v${version}`;

    const rows = platforms.map(platform => {
        const file = `[tinymist-${platform.name}.vsix](${urlBase}/tinymist-${platform.name}.vsix)`;
        const assets = platform.assets.map(asset => {
            return `[${asset.displayName}](${urlBase}/${asset.file})`;
        }).join(', ');
        return `| ${file} | ${platform.displayName} | ${assets} |`;
    });

    const table = rows.join('\n');


    return `## Download tinymist VS Code Extension ${version}
|  File  | Platform | Assets |
|--------|----------|--------|
${table}
`;
}


const collapsed = (content, summary) => {
    return `<details>

<summary><strong>${summary}</strong></summary>

${content}

</details>`;
}

const main = async () => {
    if (!versionToUpload) {
        console.error("Please provide the version to upload");
        process.exit(1);
    }

    // read version from packages.json
    const packageJson = JSON.parse(
        fs.readFileSync('./editors/vscode/package.json', 'utf8')
    );
    if (packageJson.version !== versionToUpload) {
        console.error(`Version in Cargo.toml (${packageJson.version}) is different from the version to upload (${versionToUpload})`);
        process.exit(1);
    }

    // run dist host command
    // remove target/distrib/dist-manifest.json which causes stateful announce...
    if (fs.existsSync('target/distrib/dist-manifest.json')) {
        fs.unlinkSync('target/distrib/dist-manifest.json');
    }
    
    await run(DIST_CMD + ' generate');

    const distManifest = await run(DIST_CMD + ' host --steps=upload --steps=release --output-format=json');
    const distData = JSON.parse(distManifest);
    const binInstallText = distData.announcement_github_body;
    // write to file
    fs.writeFileSync('target/announcement-dist.md', binInstallText);

    // parse-changelog .\editors\vscode\CHANGELOG.md
    const changelogPlainRaw = await run('parse-changelog ./editors/vscode/CHANGELOG.md');
    // **Full Changelog**: 
    // Patch the full changelog link
    const fullChangelogLine = /\*\*Full Changelog\*\*: https:\/\/github.com\/Myriad-Dreamin\/tinymist\/compare\/v(\d+\.\d+\.\d+)...v(\d+\.\d+\.\d+)/;
    let anyMatched = false;
    const changelogPlain = changelogPlainRaw.replace(fullChangelogLine, (_match, p1, p2) => {
        anyMatched = true;
        if (!versionToUpload.startsWith(p2)) {
            console.error(`Failed to patch the full changelog link, expected version to upload to start with ${p2}, but got ${versionToUpload}`);
            process.exit(1);
        }

        return `\*\*Full Changelog\*\*: https://github.com/Myriad-Dreamin/tinymist/compare/v${p1}...v${versionToUpload}`;
    });
    if (!anyMatched) {
        console.error("Failed to patch the full changelog link");
        process.exit(1);
    }

    fs.writeFileSync('target/announcement-changelog.md', changelogPlain);

    const extensionInstallText = generateExtensionInstall(versionToUpload);
    // concat and generate final announcement
    const binInstallSection = collapsed(binInstallText, `Download Binary`);
    const extensionInstallSection = collapsed(extensionInstallText, `Download VS Code Extension`);
    const announcement = [changelogPlain, binInstallSection, extensionInstallSection].join('\n\n');
    fs.writeFileSync('target/announcement.gen.md', announcement);

    console.log("Please check the generated announcement in target/announcement.gen.md");
};

main();
