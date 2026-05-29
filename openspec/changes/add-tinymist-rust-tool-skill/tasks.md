## 1. Author the Typst source-of-truth documents

- [ ] 1.1 Create the new `docs/tinymist/dev` source tree for the Tinymist Rust tool skill, including the top-level skill entry document and the modular API subdocuments.
- [ ] 1.2 Add Typst helpers needed to emit a standalone generated skill shape, including exact Markdown output for YAML frontmatter and other generation-sensitive fragments.
- [ ] 1.3 Draft the "how to make a tool" section with dependency import guidance for Tinymist and Typst plus concrete examples and anti-examples for word count, compile in parallel, and watch-plus-query tools.
- [ ] 1.4 Draft the "how to use Tinymist Rust APIs" section from modular Typst documents, including do and do-not guidance for recommended API layers and references to `crates/crityp` and `crates/tinymist-cli`.

## 2. Wire the generated skill into the docs workflow

- [ ] 2.1 Extend `scripts/link-docs.mjs` to generate the standalone `.codex/skills/.../SKILL.md` file from the new Typst entry document.
- [ ] 2.2 Ensure the generated skill output is emitted without the usual generated-Markdown header so the file begins with valid YAML frontmatter.
- [ ] 2.3 Keep the generation path aligned with existing repository docs commands and conventions instead of introducing a separate one-off generator.

## 3. Validate the generated skill output

- [ ] 3.1 Run the relevant docs generation command to regenerate the standalone skill and inspect the produced `SKILL.md`.
- [ ] 3.2 Verify that the generated skill is a single standalone file, starts with valid YAML frontmatter at byte zero, and preserves the required two-part structure.
- [ ] 3.3 Review the generated guidance to confirm it covers the requested scenarios, uses maintained repository examples, and does not present lower-level watcher internals as the default downstream recipe.
