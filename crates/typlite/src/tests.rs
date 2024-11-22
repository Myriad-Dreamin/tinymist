mod model;
mod rendering;

use std::sync::{LazyLock, OnceLock};

use regex::Regex;
use tinymist_world::{CompileFontArgs, EntryState, FontResolverImpl, LspUniverseBuilder};
use typst_syntax::Source;

use super::*;

fn conv_(s: &str, for_docs: bool) -> EcoString {
    static FONT_RESOLVER: LazyLock<Arc<FontResolverImpl>> = LazyLock::new(|| {
        Arc::new(
            LspUniverseBuilder::resolve_fonts(CompileFontArgs::default())
                .expect("cannot resolve default fonts"),
        )
    });

    let cwd = std::env::current_dir().unwrap();
    let main = Source::detached(s);
    let mut universe = LspUniverseBuilder::build(
        EntryState::new_rooted(cwd.as_path().into(), Some(main.id())),
        Default::default(),
        FONT_RESOLVER.clone(),
        Default::default(),
    )
    .unwrap();
    universe
        .map_shadow_by_id(main.id(), Bytes::from(main.text().as_bytes().to_owned()))
        .unwrap();
    let world = universe.snapshot();

    let converter = Typlite::new(Arc::new(world)).with_feature(TypliteFeat {
        annotate_elem: for_docs,
        ..Default::default()
    });
    let res = converter.convert().unwrap();
    static REG: OnceLock<Regex> = OnceLock::new();
    let reg = REG.get_or_init(|| Regex::new(r#"data:image/svg\+xml;base64,([^"]+)"#).unwrap());
    let res = reg.replace_all(&res, |_captures: &regex::Captures| {
        "data:image-hash/svg+xml;base64,redacted"
    });

    res.into()
}

fn conv(s: &str) -> EcoString {
    conv_(s, false)
}

fn conv_docs(s: &str) -> EcoString {
    conv_(s, true)
}

#[test]
fn test_converted() {
    insta::assert_snapshot!(conv(r###"
= Hello, World!
This is a typst document.
        "###), @r"
    # Hello, World!
    This is a typst document.
    ");
    insta::assert_snapshot!(conv(r###"
Some inlined raw `a`, ```c b```
        "###), @"Some inlined raw `a`, `b`");
    insta::assert_snapshot!(conv(r###"
- Some *item*
- Another _item_
        "###), @r"
    - Some **item**
    - Another _item_
    ");
    insta::assert_snapshot!(conv(r###"
+ A
+ B
        "###), @r"
    1. A
    1. B
    ");
    insta::assert_snapshot!(conv(r###"
2. A
+ B
        "###), @r"
    2. A
    1. B
    ");
    insta::assert_snapshot!(conv(r###"
$
1/2 + 1/3 = 5/6
$
        "###), @r#"<p align="center"><picture><source media="(prefers-color-scheme: dark)" srcset="data:image-hash/svg+xml;base64,redacted"><img alt="typst-block" src="data:image-hash/svg+xml;base64,redacted" /></picture></p>"#);
}

#[test]
fn test_converted_docs() {
    insta::assert_snapshot!(conv_docs(r###"
These again are dictionaries with the keys
- `description` (optional): The description for the argument.
- `types` (optional): A list of accepted argument types. 
- `default` (optional): Default value for this argument.

See @@show-module() for outputting the results of this function.

- content (string): Content of `.typ` file to analyze for docstrings.
- name (string): The name for the module. 
- label-prefix (auto, string): The label-prefix for internal function 
      references. If `auto`, the label-prefix name will be the module name. 
- require-all-parameters (boolean): Require that all parameters of a 
      functions are documented and fail if some are not. 
- scope (dictionary): A dictionary of definitions that are then available 
      in all function and parameter descriptions. 
- preamble (string): Code to prepend to all code snippets shown with `#example()`. 
      This can for instance be used to import something from the scope. 
-> string
        "###), @r"
    These again are dictionaries with the keys
    - <!-- typlite:begin:list-item 0 -->`description` (optional): The description for the argument.<!-- typlite:end:list-item 0 -->
    - <!-- typlite:begin:list-item 0 -->`types` (optional): A list of accepted argument types.<!-- typlite:end:list-item 0 --> 
    - <!-- typlite:begin:list-item 0 -->`default` (optional): Default value for this argument.<!-- typlite:end:list-item 0 -->

    See @@show-module() for outputting the results of this function.

    - <!-- typlite:begin:list-item 0 -->content (string): Content of `.typ` file to analyze for docstrings.<!-- typlite:end:list-item 0 -->
    - <!-- typlite:begin:list-item 0 -->name (string): The name for the module.<!-- typlite:end:list-item 0 --> 
    - <!-- typlite:begin:list-item 0 -->label-prefix (auto, string): The label-prefix for internal function 
          references. If `auto`, the label-prefix name will be the module name.<!-- typlite:end:list-item 0 --> 
    - <!-- typlite:begin:list-item 0 -->require-all-parameters (boolean): Require that all parameters of a 
          functions are documented and fail if some are not.<!-- typlite:end:list-item 0 --> 
    - <!-- typlite:begin:list-item 0 -->scope (dictionary): A dictionary of definitions that are then available 
          in all function and parameter descriptions.<!-- typlite:end:list-item 0 --> 
    - <!-- typlite:begin:list-item 0 -->preamble (string): Code to prepend to all code snippets shown with `#example()`. 
          This can for instance be used to import something from the scope.<!-- typlite:end:list-item 0 --> 
    -> string
    ");
    insta::assert_snapshot!(conv_docs(r###"
These again are dictionaries with the keys
- `description` (optional): The description for the argument.

See @@show-module() for outputting the results of this function.

- name (string): The name for the module. 
- label-prefix (auto, string): The label-prefix for internal function 
      references. If `auto`, the label-prefix name will be the module name. 
  - nested something
  - nested something 2
-> string
        "###), @r"
    These again are dictionaries with the keys
    - <!-- typlite:begin:list-item 0 -->`description` (optional): The description for the argument.<!-- typlite:end:list-item 0 -->

    See @@show-module() for outputting the results of this function.

    - <!-- typlite:begin:list-item 0 -->name (string): The name for the module.<!-- typlite:end:list-item 0 --> 
    - <!-- typlite:begin:list-item 0 -->label-prefix (auto, string): The label-prefix for internal function 
          references. If `auto`, the label-prefix name will be the module name. 
      - <!-- typlite:begin:list-item 1 -->nested something<!-- typlite:end:list-item 1 -->
      - <!-- typlite:begin:list-item 1 -->nested something 2<!-- typlite:end:list-item 1 --><!-- typlite:end:list-item 0 -->
    -> string
    ");
}
