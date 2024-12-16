// dist host --steps=upload --steps=release --output-format=json > target/dist-manifest.json

import { exec } from 'child_process';
import fs from 'fs';

const versionToUpload = process.argv[2];

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
    const distManifest = await run('dist host --steps=upload --steps=release --output-format=json');
    const distData = JSON.parse(distManifest);
    // announcement_github_body
    const body = distData.announcement_github_body;
    // write to file
    fs.writeFileSync('target/announcement-dist.md', body);

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


    // concat and generate final announcement
    const announcement = `${changelogPlain}\n\n${body}`;
    fs.writeFileSync('target/announcement.gen.md', announcement);

    console.log("Please check the generated announcement in target/announcement.gen.md");
};

main();
