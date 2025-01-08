
# Syntax Highlighting for Typst

This folder contains the syntax highlighting in the TextMate format for Typst.

To tackle challenge of making the complex grammar for typst markup, the grammar is described by neither JSON nor YAML, but a TypeScript generator program, the [./main.ts](./main.mts). TypeScript ensures correct grammar by static and strong types from [./textmate.ts](./textmate.mts).

### Building

The following script running the TypeScript program will generate the TextMate grammar file:

```shell
yarn compile
```

### Testing

```shell
// Run unit tests
yarn test
// Test on typst/typst
yarn test:official
// Test on typst/packages
yarn test:packages
```

### Register languages for raw highlighting

Goto [fenced.meta.mts](./fenced.meta.mts) and add a line like this:

```json
{ "candidates": ["erlang"] }
```

Three possible kinds:
- `{ candidates: ["someLanguage", ...rests] }` - using textmate parser registered as `source.someLanguage`.
  - The `rests` of the candidates can also be used as language tag of fenced code blocks.
- `{ as: "text.xxx", candidates }` - using textmate parser registered as `text.xxx`.
- `{ as: ["text.xxx", ...restScopes], candidates }` - using textmate parser `text.xxx` first, and `restScopes` parsers in order.

### GitHub Integration

A variant satisfying GitHub's requirement is managed on [Typst Grammar Repo](https://github.com/michidk/typst-grammar). You can check which version the repository is using by checking the [`build-ref.md`](https://github.com/michidk/typst-grammar/blob/main/build-ref.md) or [`build-ref.json`](https://github.com/michidk/typst-grammar/blob/main/build-ref.json).

The grammar is built by the [build branch's CI.](https://github.com/Myriad-Dreamin/typst-grammar/tree/build)

The grammar is tested continuously by the [main branch's CI.](https://github.com/michidk/typst-grammar/blob/main/.github/workflows/ci.yml) Specifically, it is tested by the command in the CI script:

```bash
script/grammar-compiler add vendor/grammars/typst-grammar
```

You can setup your owned environment according to [github-linguist's CONTRIBUTING.md](https://github.com/github-linguist/linguist) to develop the variant locally.

## Contributing

See [CONTRIBUTING.md](https://github.com/Myriad-Dreamin/tinymist/blob/main/CONTRIBUTING.md).
