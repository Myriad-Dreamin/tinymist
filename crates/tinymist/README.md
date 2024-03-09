
# Tinymist

This crate provides an integrated service for [Typst](https://typst.app/) [taɪpst]. It provides:
+ A language server following the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).

## Architecture

Tinymist binary has multiple modes, and it may runs multiple actors in background. The actors could run as an async task, in a single thread, or in an isolated process.

The main process of tinymist runs the program as a language server, through stdin and stdout. A main process will fork:
- rendering actors to provide PDF export with watching.
- compiler actors to provide language APIs.

## Debugging with input mirroring

You can record the input during running the editors with Tinymist. You can then replay the input to debug the language server.

```sh
# Record the input
tinymist --mirror input.txt
# Replay the input
tinymist --replay input.txt
```
