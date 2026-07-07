# AGENTS.md

This is the VS Code extension for the tinymist language service, providing language features and additional tools like preview rendering for Typst projects within the VS Code editor.

## Quick Start

1. Read `editors/vscode/package.json` and the nearest files under `src/` before editing.
2. For behavior changes, also inspect the Rust side that backs the extension, especially `crates/tinymist`, `crates/tinymist-cli`, `crates/tinymist-project`, and `crates/tinymist-query`.
3. If the task changes behavior in a non-trivial way, check OpenSpec artifacts in `openspec/specs/` and `openspec/changes/` from the repo root.
4. Before finishing, run the smallest relevant extension validation and note what you verified.

## Source Of Truth

- Edit source files under `editors/vscode/src/` for extension logic.
- Edit `editors/vscode/package.json` for extension manifest, commands, contributions, and configuration that ships in the main manifest.
- Edit `editors/vscode/package.other.json` for additional configuration definitions that are consumed by the shared docs pipeline.
- Edit `locales/tinymist-vscode.toml` and `locales/tinymist-vscode-rt.toml` for localization source text. Before that, from the repository root, run `yarn build:l10n` to extract text to translate for you.
- Edit `docs/tinymist/frontend/vscode.typ` and `docs/tinymist/config/vscode.typ` for generated VS Code docs.
- Edit `syntaxes/textmate/` for grammar changes, not the generated copies bundled into the extension.

## Do Not Hand-Edit Generated Outputs

Besides the syntax files, all other generated assets are outputs, not sources. Do not edit them directly.

- If syntax files under `editors/vscode/out/*.tmLanguage.json` need to change, update `syntaxes/textmate/` and rebuild.

## Repo Map

- `src/extension.ts`: main desktop/system extension entrypoint.
- `src/extension.web.ts`: web extension entrypoint with a reduced feature set.
- `src/extension.shared.ts`: shared activation and shutdown flow.
- `src/lsp.ts`: shared language-client state and common LSP wiring.
- `src/lsp.system.ts`: system binary launch path for the bundled or configured `tinymist` executable.
- `src/lsp.web.ts`: browser/web worker client path.
- `src/features/`: user-facing VS Code features such as preview, export, tasks, testing, labels, packages, drag-and-drop, and dev-kit integrations.
- `src/tools/`: webview and tool-panel integrations such as symbol view, docs view, templates, fonts, summaries, and profiling.
- `src/test/e2e/`: VS Code integration tests and workspace-driven scenarios.
- `e2e-workspaces/`: sample projects used by VS Code integration tests.

## Common Change Patterns

### Extension-only UI or command work

- Start in `src/extension.ts`, `src/extension.shared.ts`, `src/features/`, or `src/tools/`.
- If you add commands, menus, settings, views, or key contributions, update `package.json` too.
- If the user-facing text changes, update localization sources and regenerate localized JSON packs.

### Config work

- Check both the extension manifest side and the Rust server side.
- Server-backed settings often also require updates in `crates/tinymist/src/config.rs`.
- If configuration docs should change, update the Typst docs source and regenerate `Configuration.md`.

Example: a setting like `tinymist.serverPath` or `tinymist.fontPaths` spans `editors/vscode/package.json`, `src/config.ts`, `../../crates/tinymist/src/config.rs`, and `../../docs/tinymist/config/vscode.typ`.

### LSP or protocol work

- Extension code in `src/lsp*.ts` is often only one half of the change.
- Inspect the Rust request handler, config parser, or command implementation before assuming the fix is frontend-only.

Example: preview and export commands are surfaced through `src/lsp.ts`, but the server commands are registered on the Rust side in `../../crates/tinymist/src/server.rs` and implemented under files like `../../crates/tinymist/src/tool/preview.rs` or `../../crates/tinymist/src/task/export.rs`.

### Preview, export, or testing work

- These features often cross the TypeScript frontend and Rust backend boundary.
- Validate the specific workflow you touched, not just TypeScript unit tests.

Example: preview work commonly touches `src/features/preview.ts`, `src/lsp.ts`, `../../crates/tinymist/src/tool/preview.rs`, and an e2e file such as `src/test/e2e/simple-docs.test.ts`.

### Web vs system work

- Desktop/system mode uses `src/extension.ts` plus `src/lsp.system.ts`.
- Web mode uses `src/extension.web.ts` and supports a smaller feature set.
- If you touch activation, capability flags, or shared client code, consider whether both system and web builds still make sense.

Example: when changing startup or feature flags, compare `src/extension.ts` and `src/extension.web.ts` side by side so you do not accidentally assume browser builds support desktop-only features like the bundled binary.

## Testing

- Add or update tests for behavior changes whenever the affected area already has an established testing pattern.
- Use Vitest in `src/*.test.ts` for pure TypeScript logic that does not need a running VS Code instance.
- Use the VS Code integration harness in `src/test/e2e/*.test.ts` for commands, diagnostics, preview flows, workspace behavior, export flows, or anything that depends on editor state.
- Keep fixtures under `e2e-workspaces/<scenario>/` and make them as small and explicit as possible.
- Prefer regression-oriented assertions that exercise the public command or behavior the user actually hits.
- If an output is known to vary by environment, avoid brittle assertions and follow the existing strategy for that output type.

Examples:
- `src/language.test.ts` is the canonical unit-test pattern: pure helper imports, `vitest`, and snapshots or direct assertions.
- `src/test/e2e/simple-docs.test.ts` is the canonical command-and-preview pattern using `Context`, `suite.addTest`, `openDocument`, and `vscode.commands.executeCommand(...)`.
- `src/test/e2e/export.test.ts` shows how to build fixture-based regression checks without overasserting unstable outputs like SVG hashes.
- `src/test/e2e/diag.test.ts` is a good model when the behavior is driven by diagnostics instead of explicit commands.

## Code Style

- Follow the existing formatting and linting conventions instead of introducing a new local style.
- Let Prettier handle layout and keep ESLint-clean code under the root config in `../../eslint.config.mjs`.
- Match the existing TypeScript style in this subtree: double quotes, semicolons, and explicit imports where they improve clarity.
- Prefer early returns and small helpers for validation and control flow instead of deeply nested branches.
- Keep VS Code side effects near command handlers, activation code, or feature entrypoints; keep pure transformation logic in helpers when practical.
- Reuse existing abstractions like `tinymist`, `IContext`, the tool registry, and the e2e `Context` harness instead of inventing parallel plumbing.
- Prefix intentionally unused parameters or locals with `_` to align with the repo lint setup.
- If text is shown to users in the UI, treat localization as part of the change instead of hardcoding new strings in only one place.

Examples:
- `src/context.ts` shows the common guard-and-early-return style used around editor state and command helpers.
- `src/package-manager.ts` is a good example of small validation helpers and straightforward control flow around `showInputBox` and `showQuickPick`.
- `src/features/export.ts` shows the preferred pattern of keeping command registration near the feature boundary and pushing reusable logic into helper functions.

## Validation

Run the narrowest command that covers your change, then widen if the change crosses boundaries.

- TypeScript only: `yarn type-check`
- Formatting: `yarn format-check`
- Linting: `cd ../.. && yarn lint`
- Unit tests: `yarn test:unit`
- VS Code integration tests: `yarn test:vsc`
- Full extension test pass: `yarn test`
- Rebuild desktop/system extension bundle: `yarn compile:system`
- Rebuild web extension bundle: `yarn compile:web`
- Regenerate localization packs from source: `cd ../.. && yarn build:l10n`
- Regenerate syntax assets: `cd ../.. && yarn build:syntax`
- Verify generated docs are current: `cd ../.. && node scripts/link-docs.mjs --check`

Examples:
- Command or menu change with no backend work: start with `yarn type-check` and `yarn test:unit`.
- Workspace behavior or preview change: prefer `yarn test:vsc`.
- TypeScript refactor or style cleanup: include `cd ../.. && yarn lint` and `yarn format-check`.
- Manifest, docs, or localization change: also run `cd ../.. && yarn build:l10n` or `cd ../.. && node scripts/link-docs.mjs --check` as appropriate.
- Grammar change: run `cd ../.. && yarn build:syntax` and `cd ../.. && yarn test:grammar`.

## Practical Rules

- Prefer changing the real source instead of patching generated assets under `out/`, `test-dist/`, or generated Markdown.
- Keep changes consistent across manifest, localization, docs, and backend config when the feature spans those layers.
- Do not assume the extension can fix a server-side behavior problem by itself. Check the Rust implementation before locking into a frontend-only solution.
- Preserve feature differences between web and system builds unless the task explicitly changes that boundary.
- When tests use workspace fixtures, update or add the smallest scenario under `e2e-workspaces/` that proves the behavior.

Examples:
- Good: update `package.json`, `src/config.ts`, and `../../crates/tinymist/src/config.rs` together for a real config change.
- Bad: patch `out/extension.js`, `package.nls.json`, or `README.md` directly because the visible output is wrong.
- Good: add a focused `e2e-workspaces/` fixture and one `src/test/e2e/*.test.ts` assertion for a regression.
