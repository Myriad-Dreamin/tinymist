#import "mod.typ": *

#show: book-page.with(title: [Testing Feature])

The testing feature is available since `tinymist` v0.13.10.

== IDE Support

You can run tests and check coverage in the IDE or CLI.

== Test Discovery <tinymist-test-discovery>

Given a file, tinymist will try to discover tests related to the file.
- All dependent files in the same workspace will be checked.
  - For example, if file `a.typ` contains `import "b.typ"` or `include "b.typ"`, tinymist will check `b.typ` for tests as well.
- For each file including the entry file itself, tinymist will check the file for tests.
  - If a file is named `example-*.typ`, it is considered an *example document* and will be compiled using `typst::compile`.
    - Both png export and html export may be called.
    - For now, png export is always called for each example file.
    - If the label `<test-html-example>` can be found in the example file, html export will be called.
  - Top-level functions will be checked for tests.
    - If a function is named `test-*`, it is considered a test function and will be called directly.
    - If a function is named `bench-*`, it is considered a benchmark function and will be called once to collect coverage.
    - If a function is named `panic-on-*`, it will only pass the test if a panic occurs during execution.

Example Entry File:
```typ
#import "example-hello-world.typ"

#let test-it() = {
  "test"
}

#let panic-on-panic() = {
  panic("this is a panic")
}
```

Example Output:
```
Found 2 tests and 1 examples
Running test(test-it)
Running test(panic-on-panic)
 Passed test(test-it)
 Passed test(panic-on-panic)
Running example(example-hello-world
 Failed example(example-hello-world): image mismatch
   Hint example(example-hello-world): compare image at refs/png/example-hello-world.png
 Passed example(example-hello-world)
  Info: Written coverage to target/coverage.json ...
 Fatal: Some test cases failed...
```

== Benchmarking

Since it requires some heavy framework to run benchmarks, a standalone tool is provided to run benchmarks.

Check #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/crityp")[crityp] for more information.

== Visualizing Coverage

- Run and collect file coverage using command `tinymist.profileCurrentFileCoverage` in VS Cod(e,ium).
- Run and collect test coverage using command `tinymist.profileCurrentTestCoverage` in VS Cod(e,ium).
  - Check #link(<tinymist-test-discovery>)[Test Discovery] to learn how tinymist discovers tests.

VS Cod(e,ium) will show the overall coverage in the editor.

== CLI Support

You can run tests and check coverage in the CLI.

```bash
tinymist test tests/main.typ
...
  Info: All test cases passed...
```

You can pass same arguments as `typst compile` to `tinymist test`.

== Debugging tests with CLI

If any test fails, the CLI will return a non-zero exit code.

```bash
tinymist test tests/main.typ
...
 Fatal: Some test cases failed...
```

To update the reference files, you can run:

```bash
tinymist test tests/main.typ --update
```

To get image files to diff you can use grep to find the image files to update:

```bash
tinymist test tests/main.typ 2> >(grep Hint) > >(grep "compare image")
   Hint example(example-hello-world): compare image at target/refs/png/example-hello-world.png
   Hint example(example-other): compare image at target/refs/png/example-other.png
```

You can use your favorite image `diff` tool to compare the images, e.g. `magick compare`.

== Tips: Reproducible Rendering

To ensure that the rendering is reproducible, you can ignore system fonts.

```bash
tinymist test tests/main.typ --ignore-system-fonts
```

Adds font paths using the `--font-paths` option if you want to use custom fonts:

```bash
tinymist test tests/main.typ --font-paths /path/to/fonts
```

== Continuous Integration

`tinymist test` only compares hash files to check whether content is changed. Therefore, you can ignore rendered files and only keep the hash files to compare them on CI. Putting the following content in `.gitignore` will help you to ignore the files:

```exclude
# png files
refs/png/**/*.png
# html files
refs/html/**/*.html
# hash files
!refs/**/*.hash
```

Install `tinymist` on CI and run `tinymist test` to check whether the content is changed.

```yaml
- name: Install tinymist
  env:
    TINYMIST_VERSION: 0.13.x # to test with typst compiler v0.13.x, tinymist v0.14.x for typst v0.14.x, and so on.
  run: curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Myriad-Dreamin/tinymist/releases/download/${TINYMIST_VERSION}/tinymist-installer.sh | sh
- name: Run tests (Typst)
  run: tinymist test tests/main.typ --root . --ppi 144 --ignore-system-fonts
- name: Upload artifacts
  uses: actions/upload-artifact@v4
  with:
    name: refs
    path: refs
```
