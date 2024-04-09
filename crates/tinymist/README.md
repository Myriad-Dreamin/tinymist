
# tinymist

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
tinymist lsp --mirror input.txt
# Replay the input
tinymist lsp --replay input.txt
```

## Analyze memory usage with DHAT

You can build the program with `dhat-heap` feature to collect memory usage with DHAT. The DHAT will instrument the allocator dynamically, so it will slow down the program significantly.

```sh
cargo build --release --bin tinymist --features dhat-heap
```

The instrumented program is nothing different from the normal program, so you can mine the specific memory usage with a lsp session (recorded with `--mirror`) by replaying the input.

```sh
./target/release/tinymist lsp --replay input.txt
...
dhat: Total:     740,668,176 bytes in 1,646,987 blocks
dhat: At t-gmax: 264,604,009 bytes in 317,241 blocks
dhat: At t-end:  259,597,420 bytes in 313,588 blocks
dhat: The data has been saved to dhat-heap.json, and is viewable with dhat/dh_view.html
```

Once you have the `dhat-heap.json`, you can visualize the memory usage with [the DHAT viewer](https://nnethercote.github.io/dh_view/dh_view.html).

## Contributing

See [CONTRIBUTING.md](https://github.com/Myriad-Dreamin/tinymist/blob/main/CONTRIBUTING.md).
