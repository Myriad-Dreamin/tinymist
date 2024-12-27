
# `tinymist-query` analyzer library

## Documentation

The documentation can be generated locally by running:

```bash
yarn docs:rs --open # You can also copy-run the command if you don't have a yarn.
```

## Name Convention

The names of analyzers are all have suffix `Worker`. To get a full list of analyzers, please search "struct: tinymist_query worker" in Cargo Docs.

```rust
tinymist_query::analysis::bib::BibWorker
tinymist_query::syntax::expr::ExprWorker
tinymist_query::syntax::index::IndexWorker
tinymist_query::analysis::link_expr::LinkStrWorker
tinymist_query::analysis::OnEnterWorker
tinymist_query::analysis::color_expr::ColorExprWorker
tinymist_query::analysis::InlayHintWorker
...
```

## Testing analyzers

To run analyzer tests for tinymist:

```bash
cargo insta test -p tinymist-query --accept
```

> [!Tip]  
> Check [Cargo Insta](https://insta.rs/docs/cli/) to learn and install the `insta` command.

To add a test, for example for the [`textDocument/completion`](/crates/tinymist-query/src/completion.rs) function, please follow the steps:

1. Check the first argument of `snapshot_testing` function calls in the source file. We know that it uses fixtures from [`fixtures/completion/`](/crates/tinymist-query/src/fixtures/completion/).
2. Add a typst file in the [`fixtures/completion/`](/crates/tinymist-query/src/fixtures/completion/) directory.
3. Rerun the `cargo insta` command to update snapshot.

At the step 2, we have special syntax to write the typst file.

1. Cursor range to request completion: `/* range -1..0 */` or `/* range after 1..2 */` that relative to the comment node.

    Example:

    ```typ
    $: /* range -1..0 */$
    ```

    It test completion when the cursor is after a colon in the equation context.
2. Completion result filter: `/// contains: comma separated list`

    Example:
    
    ```typ
    /// contains: body, fill
    #set text(/* range 0..1 */ )
    ```

    Description: The snapshot result **must** contains `fill` and **must not** contains `body`, so we add both of them to the `contains` list.

3. Fixture of some multiple-files project:  `/// path: file name`

    Example:

    ```typ
    /// path: base.typ
    #let table-prefix() = 1;

    -----
    /// contains: base,table,table-prefix
    #import "base.typ": table/* range -1..1 */
    ```

    Description: the files are separated by `-----`, and the first file is named `base.typ`. The snapshot result **must not** contain `base`, `table`, and **must** contain `table-prefix`, so we add them to the `contains` list.

4. Whether to compile the file: `/// compile: true`

    Example:

    ```typ
    /// compile: true

    #let test1(body) = figure(body)
    #test1([Test1]) <fig:test1>
    /* position after */ @fig:test1
    ```

    Description: The file will be compiled before the test, to get the content reference by `typst::trace` for completion.

4. Compile which the file: `/// compile: base.typ`

    Example:

    ```typ
    /// compile: true

   /// path: base.typ
    #set heading(numbering: "1.1")

    = H <t>

    == H2 <test>

    aba aba

    -----
    /// contains: test
    /// compile: base.typ

    #<t /* range -2..-1 */
    ```

    Description: Another file named `base.typ` will be compiled before the test, since a half-editing file causes the completion to fail.

## Analyzer playground

A special fixture folder [`playground`](/crates/tinymist-query/src/fixtures/playground/) is provided for testing analyzers. You can change argument of the `snapshot_testing` to `"playground"` or `""` to use the playground fixtures.
