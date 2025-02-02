# tinymist-world

Typst's World implementation for [tinymist.](https://github.com/Myriad-Dreamin/tinymist)

### Example: Resolves a system universe from system arguments

```rust
let args = CompileOnceArgs::parse();
let universe = args
    .resolve_system()
    .expect("failed to resolve system universe");
```

### Example: Runs a typst compilation

```rust
let world = verse.snapshot();
// in current thread
let doc = typst::compile(&world)?;
// the snapshot is Send + Sync
std::thread::spawn(move || {
    let doc = typst::compile(&world)?;
});
```
