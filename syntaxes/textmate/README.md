
# Syntax Highlighting for Typst

This folder contains the syntax highlighting for Typst. The syntax highlighting is written in the TextMate format.

The syntax highlighting is written in TypeScript, and ensures correct grammar by [./textmate.ts](./textmate.mts).

### Building

The following script running the TypeSCript program will generate the TextMate grammar file:

```shell
yarn compile
```

### Testing

```shell
yarn test
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

## Contributing

See [CONTRIBUTING.md](https://github.com/Myriad-Dreamin/tinymist/blob/main/CONTRIBUTING.md).
