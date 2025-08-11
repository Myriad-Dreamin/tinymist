#!/usr/bin/env node

import fs from 'fs/promises';
import path from 'path';
import { fileURLToPath } from 'url';
import init, { edit, parse, stringify } from "@rainbowatcher/toml-edit-js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.dirname(__dirname);

class NightlyUtils {
    constructor(rootDir = ROOT_DIR) {
        this.rootDir = rootDir;
        this.initialized = false;
    }

    async ensureInit() {
        if (!this.initialized) {
            await init();
            this.initialized = true;
        }
    }

    async readToml(filePath) {
        const content = await fs.readFile(filePath, 'utf-8');
        return { content, parsed: parse(content) };
    }

    async writeToml(filePath, content) {
        await fs.writeFile(filePath, content);
    }

    async readJson(filePath) {
        const content = await fs.readFile(filePath, 'utf-8');
        return JSON.parse(content);
    }

    async writeJson(filePath, data) {
        const content = JSON.stringify(data, null, 2) + '\n';
        await fs.writeFile(filePath, content);
    }

    async getCurrentDependencyRevs() {
        await this.ensureInit();
        const cargoTomlPath = path.join(this.rootDir, 'Cargo.toml');
        const { parsed } = await this.readToml(cargoTomlPath);

        const patches = parsed?.patch?.['crates-io'] || {};

        const extractRev = (depInfo) => {
            if (typeof depInfo === 'string') return null;
            if (typeof depInfo === 'object' && depInfo.rev) return depInfo.rev;
            return null;
        };

        return {
            typst: extractRev(patches.typst),
            reflexo: extractRev(patches.reflexo),
            typstyle: extractRev(patches['typstyle-core']),
            'typst-ansi-hl': extractRev(patches['typst-ansi-hl'])
        };
    }

    async updateDependencies(crates, version) {
        await this.ensureInit();
        const cargoTomlPath = path.join(this.rootDir, 'Cargo.toml');
        const { content } = await this.readToml(cargoTomlPath);

        let updatedContent = content;

        const updateCrateDependencyVersion = (content, crate, newVersion) => {
            const parsed = parse(content)
            const deps = parsed?.workspace?.dependencies || {};

            if (!(crate in deps)) {
                throw Error("Missing package")
            }

            const crateDepInfo = deps[crate];

            if (typeof crateDepInfo === 'string') {
                return edit(content, `workspace.dependencies.${crate}`, newVersion)
            }
            if (typeof crateDepInfo === 'object' && crateDepInfo.version) {
                return edit(content, `workspace.dependencies.${crate}.version`, newVersion)
            }

            throw Error("Invalid dependency info")
        }

        for (const crate of crates) {
            try {
                updatedContent = updateCrateDependencyVersion(updatedContent, crate, version)
            } catch (e) {
            }
        }

        await this.writeToml(cargoTomlPath, updatedContent);
    }

    async updateTypstDependencies(typstVersion, typstAssetsRev) {
        await this.ensureInit();

        const typstCrates = [
            'typst-cli',
            'typst-eval',
            'typst-html',
            'typst-ide',
            'typst-kit',
            'typst-layout',
            'typst-library',
            'typst-macros',
            'typst-pdf',
            'typst-realize',
            'typst-render',
            'typst-svg',
            'typst-syntax',
            'typst-timing',
            'typst-utils',
            'typst',
        ];
        await this.updateDependencies(typstCrates, typstVersion)

        const cargoTomlPath = path.join(this.rootDir, 'Cargo.toml');
        const { content } = await this.readToml(cargoTomlPath);

        let updatedContent = content;

        try {
            updatedContent = edit(updatedContent, 'workspace.dependencies.typst-assets.rev', typstAssetsRev);
        } catch (e) {
            console.warn(`Warning: Could not update typst - assets rev: ${e.message} `);
        }

        await this.writeToml(cargoTomlPath, updatedContent);
    }

    async bumpWorldCrates(newVersion, bump = false) {
        await this.ensureInit();

        const worldCrates = [
            'tinymist-derive', 'tinymist-l10n', 'tinymist-package', 'tinymist-std',
            'tinymist-vfs', 'tinymist-world', 'tinymist-project', 'tinymist-task', 'typst-shim'
        ];
        await this.updateDependencies(worldCrates, newVersion);

        if (!bump) {
            return
        }

        for (const crate of worldCrates) {
            const cratePath = path.join(this.rootDir, 'crates', crate, 'Cargo.toml');
            try {
                const { content: crateContent } = await this.readToml(cratePath);
                const updatedCrateContent = edit(crateContent, 'package.version', newVersion);
                await this.writeToml(cratePath, updatedCrateContent);
            } catch (e) {
                console.warn(`Warning: Could not update ${crate}/Cargo.toml: ${e.message}`);
            }
        }
    }

    async updatePatchRevs(revs) {
        await this.ensureInit();
        const cargoTomlPath = path.join(this.rootDir, 'Cargo.toml');
        const { content, parsed } = await this.readToml(cargoTomlPath);

        let updatedContent = content;

        const patchMappings = [
            { key: 'reflexo', patches: ['reflexo', 'reflexo-typst', 'reflexo-vec2svg'] },
            { key: 'typst-ansi-hl', patches: ['typst-ansi-hl'] },
            { key: 'typstyle', patches: ['typstyle-core'] },
            {
                key: 'typst', patches: [
                    'typst-cli',
                    'typst-eval',
                    'typst-html',
                    'typst-ide',
                    'typst-kit',
                    'typst-layout',
                    'typst-library',
                    'typst-macros',
                    'typst-pdf',
                    'typst-realize',
                    'typst-render',
                    'typst-svg',
                    'typst-syntax',
                    'typst-timing',
                    'typst-utils',
                    'typst',
                ]
            },
            {
                key: 'tinymist', patches: [
                    'crityp',
                    'tinymist',
                    'tinymist-assets',
                    'tinymist-dap',
                    'tinymist-derive',
                    'tinymist-lint',
                    'tinymist-project',
                    'tinymist-render',
                    'tinymist-task',
                    'tinymist-vfs',
                    'typlite',
                    'typst-shim',
                    'sync-lsp',
                    'tinymist',
                    'tinymist-analysis',
                    'tinymist-debug',
                    'tinymist-l10n',
                    'tinymist-package',
                    'tinymist-query',
                    'tinymist-std',
                    'tinymist-tests',
                    'tinymist-world',
                    'typst-preview',
                ]
            },
        ];

        for (const mapping of patchMappings) {
            if (revs[mapping.key]) {
                for (const patchName of mapping.patches) {
                    try {
                        let patchInfo = parsed?.patch?.['crates-io'][patchName] || null
                        if (!patchInfo) {
                            continue;
                        }

                        delete patchInfo.branch;
                        delete patchInfo.tag;

                        patchInfo.rev = revs[mapping.key];
                        updatedContent = edit(updatedContent, `patch.crates-io.${patchName}`, patchInfo);
                    } catch (e) {
                        console.warn(`Warning: Could not update ${patchName} rev: ${e.message}`);
                    }
                }
            }
        }

        await this.writeToml(cargoTomlPath, updatedContent);
    }

    async updateMainVersion(newVersion) {
        await this.ensureInit();

        const nonWorldCrates = [
            'sync-ls', 'tinymist', 'tinymist-analysis', 'tinymist', 'tinymist-debug',
            'tinymist-lint', 'tinymist-query', 'tinymist-render', 'tinymist-preview', 'typlite'
        ];
        await this.updateDependencies(nonWorldCrates, newVersion);

        const cargoTomlPath = path.join(this.rootDir, 'Cargo.toml');
        const { content } = await this.readToml(cargoTomlPath);

        let updatedContent = edit(content, 'workspace.package.version', newVersion);

        await this.writeToml(cargoTomlPath, updatedContent);
    }

    async updateVersionFiles(newVersion) {
        const jsonFiles = [
            'contrib/html/editors/vscode/package.json',
            'crates/tinymist/package.json',
            'editors/vscode/package.json',
            'syntaxes/textmate/package.json'
        ];

        for (const file of jsonFiles) {
            const filePath = path.join(this.rootDir, file);
            try {
                const json = await this.readJson(filePath);
                json.version = newVersion;
                await this.writeJson(filePath, json);
            } catch (e) {
                console.warn(`Warning: Could not update ${file}: ${e.message}`);
            }
        }

        await this.updateSpecialFiles(newVersion);
    }

    async updateSpecialFiles(newVersion) {
        // Nix flake
        try {
            const nixFlakePath = path.join(this.rootDir, 'contrib/nix/dev/flake.nix');
            let nixContent = await fs.readFile(nixFlakePath, 'utf-8');
            nixContent = nixContent.replace(
                /version = "[^"]*";/g,
                `version = "${newVersion}";`
            );
            await fs.writeFile(nixFlakePath, nixContent);
        } catch (e) {
            console.warn(`Warning: Could not update flake.nix: ${e.message}`);
        }

        // Dockerfile
        try {
            const dockerfilePath = path.join(this.rootDir, 'editors/neovim/samples/lazyvim-dev/Dockerfile');
            let dockerContent = await fs.readFile(dockerfilePath, 'utf-8');
            dockerContent = dockerContent.replace(
                /FROM myriaddreamin\/tinymist:[^ ]* as tinymist/g,
                `FROM myriaddreamin/tinymist:${newVersion} as tinymist`
            );
            await fs.writeFile(dockerfilePath, dockerContent);
        } catch (e) {
            console.warn(`Warning: Could not update Dockerfile: ${e.message}`);
        }

        // bootstrap.sh
        try {
            const bootstrapPath = path.join(this.rootDir, 'editors/neovim/bootstrap.sh');
            let bootstrapContent = await fs.readFile(bootstrapPath, 'utf-8');
            bootstrapContent = bootstrapContent.replace(
                /myriaddreamin\/tinymist:[^ ]*/g,
                `myriaddreamin/tinymist:${newVersion}`
            );
            bootstrapContent = bootstrapContent.replace(
                /myriaddreamin\/tinymist-nvim:[^ ]*/g,
                `myriaddreamin/tinymist-nvim:${newVersion}`
            );
            await fs.writeFile(bootstrapPath, bootstrapContent);
        } catch (e) {
            console.warn(`Warning: Could not update bootstrap.sh: ${e.message}`);
        }
    }

    async generateChangelog(newVersion, tinymistBaseCommit, tinymistBaseMessage, typstRev, typstBaseCommit, typstBaseMessage) {
        const currentDate = new Date().toISOString().split('T')[0];
        const changelogPath = path.join(this.rootDir, 'editors/vscode/CHANGELOG.md');

        // Template for the new changelog entry.
        const newChangelogEntryTemplate = `## v${newVersion} - [${currentDate}]

Nightly Release at [${tinymistBaseMessage}](https://github.com/Myriad-Dreamin/tinymist/commit/${tinymistBaseCommit}), using [ParaN3xus/typst rev ${typstRev.slice(0, 7)}](https://github.com/ParaN3xus/typst/commit/${typstRev}), a.k.a. [typst/typst ${typstBaseMessage}](https://github.com/typst/typst/commit/${typstBaseCommit}).

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/{{PREV_VERSION}}...v${newVersion}
`;

        try {
            const content = await fs.readFile(changelogPath, 'utf-8');
            const lines = content.split('\n');

            const newVersionBase = `v${newVersion.split('-')[0]}`;

            const filteredLines = [];
            let isSkipping = false;

            // skip lines with same base ver
            for (const line of lines) {
                if (line.startsWith('## v')) {
                    const match = line.match(/## (v[0-9]+\.[0-9]+\.[0-9]+)/);
                    if (match && match[1] === newVersionBase) {
                        isSkipping = true;
                    } else {
                        isSkipping = false;
                    }
                }

                if (!isSkipping) {
                    filteredLines.push(line);
                }
            }

            // find insert point
            let previousVersion = '';
            for (const line of filteredLines) {
                if (line.startsWith('## v')) {
                    const match = line.match(/## (v[0-9][^\s]*)/);
                    if (match) {
                        previousVersion = match[1];
                        break;
                    }
                }
            }

            let finalEntry = newChangelogEntryTemplate.replace(
                '{{PREV_VERSION}}',
                previousVersion || 'HEAD~1'
            );

            const firstReleaseIndex = filteredLines.findIndex(line => line.startsWith('## v'));

            let finalContent;
            if (firstReleaseIndex >= 0) {
                const newLines = [
                    ...filteredLines.slice(0, firstReleaseIndex),
                    finalEntry,
                    ...filteredLines.slice(firstReleaseIndex)
                ];
                finalContent = newLines.join('\n');
            } else {
                // append
                finalContent = filteredLines.join('\n').trim() + '\n\n' + finalEntry;
            }

            await fs.writeFile(changelogPath, finalContent.trim() + '\n');

        } catch (e) {
            // create
            console.warn(`Warning: Could not update CHANGELOG.md: ${e.message}. Creating a new one.`);
            const finalEntry = newChangelogEntryTemplate.replace('{{PREV_VERSION}}', 'HEAD~1');
            await fs.writeFile(changelogPath, finalEntry);
        }
    }

    calculateNewVersion(currentVersion, releaseType) {
        const validVersionRegex = /^[0-9.\-rc]+$/;
        if (!validVersionRegex.test(currentVersion)) {
            throw new Error(`Invalid version format: ${currentVersion}. Version can only contain numbers, dots, hyphens, and 'rc'.`);
        }

        if (releaseType === 'canary') {
            const [baseVersion, suffix] = currentVersion.split('-');
            const versionParts = baseVersion.split('.');
            const currentPatch = parseInt(versionParts[2]);

            if (currentPatch % 2 === 0) {
                // even -> +1-rc1
                const newPatch = currentPatch + 1;
                const newVersion = `${versionParts[0]}.${versionParts[1]}.${newPatch}`;
                return `${newVersion}-rc1`;
            } else {
                // odd
                if (suffix && suffix.startsWith('rc')) {
                    const rcNumber = parseInt(suffix.replace('rc', ''));

                    if (rcNumber === 9) {
                        // rc9 -> +1-rc1
                        const newPatch = currentPatch + 2;
                        const newVersion = `${versionParts[0]}.${versionParts[1]}.${newPatch}`;
                        return `${newVersion}-rc1`;
                    } else {
                        // rc -> rc+1
                        const newRcNumber = rcNumber + 1;
                        return `${baseVersion}-rc${newRcNumber}`;
                    }
                } else {
                    // no rc -> +2-rc1
                    const newPatch = currentPatch + 2;
                    const newVersion = `${versionParts[0]}.${versionParts[1]}.${newPatch}`;
                    return `${newVersion}-rc1`;
                }
            }
        } else {
            // nightly release
            // simply remove -rc
            const baseVersion = currentVersion.split('-')[0];
            const versionParts = baseVersion.split('.');
            const currentPatch = parseInt(versionParts[2]);

            if (currentPatch % 2 === 0) {
                throw new Error(`Current patch version ${currentPatch} is not odd. Nightly releases require odd patch versions.`);
            }

            return baseVersion;
        }
    }

}

async function main() {
    const rootDir = process.argv[2];

    const utils = new NightlyUtils(rootDir);

    const command = process.argv[3];

    if (!command) {
        console.error('Please specify a command');
        process.exit(1);
    }

    try {
        switch (command) {
            case 'get-current-revs': {
                const revs = await utils.getCurrentDependencyRevs();
                Object.entries(revs).forEach(([key, value]) => {
                    console.log(`current_${key.replaceAll('-', '_')}_rev=${value || ''}`);
                });
                break;
            }

            case 'update-typst-deps': {
                const typstVersion = process.argv[4];
                const assetsRev = process.argv[5];
                if (!typstVersion || !assetsRev) {
                    throw new Error('Usage: update-typst-deps <typst-version> <assets-rev>');
                }
                await utils.updateTypstDependencies(typstVersion, assetsRev);
                console.log(`Updated typst dependencies to ${typstVersion}`);
                break;
            }

            case 'update-world-crates': {
                const newVersion = process.argv[4];
                if (!newVersion) {
                    throw new Error('Usage: update-world-crates <new-version>');
                }
                await utils.bumpWorldCrates(newVersion, false);
                console.log(`Updated world crates to ${newVersion}`);
                break;
            }

            case 'bump-world-crates': {
                const newVersion = process.argv[4];
                if (!newVersion) {
                    throw new Error('Usage: bump-world-crates <new-version>');
                }
                await utils.bumpWorldCrates(newVersion, true);
                console.log(`Updated world crates to ${newVersion}`);
                break;
            }

            case 'update-patch-revs': {
                const revsJson = process.argv[4];
                if (!revsJson) {
                    throw new Error('Usage: update-patch-revs <revs-json>');
                }
                const revs = JSON.parse(revsJson);
                await utils.updatePatchRevs(revs);
                console.log('Updated patch revisions');
                break;
            }

            case 'update-main-version': {
                const newVersion = process.argv[4];
                if (!newVersion) {
                    throw new Error('Usage: update-main-version <new-version>');
                }
                await utils.updateMainVersion(newVersion);
                console.log(`Updated main version to ${newVersion}`);
                break;
            }

            case 'update-version-files': {
                const newVersion = process.argv[4];
                if (!newVersion) {
                    throw new Error('Usage: update-version-files <new-version>');
                }
                await utils.updateVersionFiles(newVersion);
                console.log(`Updated version files to ${newVersion}`);
                break;
            }

            case 'generate-changelog': {
                const newVersion = process.argv[4];
                const tinymistBaseCommit = process.argv[5];
                const tinymistBaseMessage = process.argv[6];
                const typstRev = process.argv[7];
                const typstBaseCommit = process.argv[8];
                const typstBaseMessage = process.argv[9];
                if (!newVersion || !tinymistBaseCommit || !tinymistBaseMessage || !typstRev || !typstBaseCommit || !typstBaseMessage) {
                    throw new Error('Usage: generate-changelog <version> <tinymist-base-commit> <tinymist-base-message> <typst-rev> <typst-base-commit> <typst-base-message>');
                }
                await utils.generateChangelog(newVersion, tinymistBaseCommit, tinymistBaseMessage, typstRev, typstBaseCommit, typstBaseMessage);
                console.log('Generated changelog');
                break;
            }

            case 'calculate-version': {
                const currentVersion = process.argv[4];
                const releaseType = process.argv[5];
                if (!currentVersion || !releaseType) {
                    throw new Error('Usage: calculate-version <current-version> <release-type>');
                }
                const newVersion = utils.calculateNewVersion(currentVersion, releaseType);
                console.log(newVersion);
                break;
            }

            default:
                console.error(`Unknown command: ${command}`);
                console.error('Available commands:');
                console.error('  get-current-revs');
                console.error('  update-typst-deps <typst-version> <assets-rev>');
                console.error('  bump-world-crates <new-version>');
                console.error('  update-world-crates <new-version>');
                console.error('  update-patch-revs <revs-json>');
                console.error('  update-main-version <new-version>');
                console.error('  update-version-files <new-version>');
                console.error('  generate-changelog <version> <tinymist-base-commit> <tinymist-base-message> <typst-rev> <typst-base-commit> <typst-base-message>');
                console.error('  calculate-version <current-version> <release-type>');
                process.exit(1);
        }
    } catch (error) {
        console.error('Error:', error.message);
        process.exit(1);
    }
}

if (import.meta.url === `file://${process.argv[1]}`) {
    main();
}

export default NightlyUtils;
