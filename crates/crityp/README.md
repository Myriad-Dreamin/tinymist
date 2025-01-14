# Crityp

Benchmark support for typst.

## Usage

Use `crityp` to benchmark a typst file. The CLI arguments is compatible with `typst-cli compile`.

```shell
crityp test-bench.typ
Benchmarking /test-bench.typ@bench-fib
Benchmarking /test-bench.typ@bench-fib: Warming up for 3.0000 s
Benchmarking /test-bench.typ@bench-fib: Collecting 100 samples in estimated 5.3151 s (56k iterations)
Benchmarking /test-bench.typ@bench-fib: Analyzing
/test-bench.typ@bench-fib
                        time:   [93.919 µs 94.631 µs 95.459 µs]
                        change: [-7.2275% -5.2111% -3.4660%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  2 (2.00%) low mild
  4 (4.00%) high mild
  1 (1.00%) high severe
```
