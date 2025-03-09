use std::sync::Arc;

use tinymist::{project::LspUniverseBuilder, Config};

fn main() {
    // initialize global variables

    // Run registered benchmarks.
    divan::main();
}

// Checks font loading performance of embedded fonts
#[divan::bench]
fn load_embedded() {
    let _embedded_fonts = Arc::new(LspUniverseBuilder::only_embedded_fonts().unwrap());
}

// Checks font loading performance of system fonts
#[divan::bench]
fn load_system() {
    let config = Config::default();

    let font_resolver = config.compile.determine_fonts();

    let _font = font_resolver.wait().clone();
}

/*
No par
Timer precision: 17 ns
tinymist_bench_font_load  fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ load_embedded          1.167 ms      │ 1.697 ms      │ 1.176 ms      │ 1.188 ms      │ 100     │ 100
╰─ load_system            111.8 ms      │ 123 ms        │ 113.6 ms      │ 114.3 ms      │ 100     │ 100

Par
Timer precision: 17 ns
tinymist_bench_font_load  fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ load_embedded          131.4 µs      │ 894.4 µs      │ 153.5 µs      │ 164.9 µs      │ 100     │ 100
╰─ load_system            -

 */
