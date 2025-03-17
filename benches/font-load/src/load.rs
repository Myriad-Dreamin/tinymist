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

    let _fonts = config.fonts();
}

/*
Without Parallelization
Timer precision: 17 ns
tinymist_bench_font_load  fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ load_embedded          1.167 ms      │ 1.697 ms      │ 1.176 ms      │ 1.188 ms      │ 100     │ 100
╰─ load_system            111.8 ms      │ 123 ms        │ 113.6 ms      │ 114.3 ms      │ 100     │ 100

With Parallelization
Timer precision: 17 ns
tinymist_bench_font_load  fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ load_embedded          130.8 µs      │ 1.164 ms      │ 157 µs        │ 170.3 µs      │ 100     │ 100
╰─ load_system            14.44 ms      │ 18.22 ms      │ 15.37 ms      │ 15.54 ms      │ 100     │ 100

 */
