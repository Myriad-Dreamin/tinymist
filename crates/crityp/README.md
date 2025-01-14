# Crityp

Benchmark support for typst.

## Usage

```shell
crityp test-bench.typ@fib-bench
Benchmarking fib-bench typst
Benchmarking fib-bench typst: Warming up for 3.0000 s
Benchmarking fib-bench typst: Collecting 100 samples in estimated 5.2594 s (56k iterations)
Benchmarking fib-bench typst: Analyzing
test-bench.typ@fib-bench time:   [93.551 µs 94.220 µs 94.967 µs]    
                         change: [-91.268% -90.867% -90.520%] (p = 0.00 < 0.05)
                         Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  3 (3.00%) high mild
  1 (1.00%) high severe
```
