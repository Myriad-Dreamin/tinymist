mod model;
mod rendering;

use std::sync::{LazyLock, OnceLock};

use regex::Regex;
use tinymist_world::{CompileFontArgs, EntryState, FontResolverImpl, LspUniverseBuilder};
use typst_syntax::Source;

use super::*;

fn conv(s: &str) -> EcoString {
    static FONT_RESOLVER: LazyLock<Result<Arc<FontResolverImpl>>> = LazyLock::new(|| {
        Ok(Arc::new(
            LspUniverseBuilder::resolve_fonts(CompileFontArgs::default())
                .map_err(|e| format!("{e:?}"))?,
        ))
    });

    let font_resolver = FONT_RESOLVER.clone();
    let cwd = std::env::current_dir().unwrap();
    let main = Source::detached(s);
    let mut universe = LspUniverseBuilder::build(
        EntryState::new_rooted(cwd.as_path().into(), Some(main.id())),
        font_resolver.unwrap(),
        Default::default(),
        Default::default()
    )
    .unwrap();
    universe
        .map_shadow_by_id(main.id(), Bytes::from(main.text().as_bytes().to_owned()))
        .unwrap();
    let world = universe.snapshot();

    let res = Typlite::new(Arc::new(world)).convert().unwrap();
    static REG: OnceLock<Regex> = OnceLock::new();
    let reg = REG.get_or_init(|| Regex::new(r#"data:image/svg\+xml;base64,([^"]+)"#).unwrap());
    let res = reg.replace(&res, |_captures: &regex::Captures| {
        // let hash = _captures.get(1).unwrap().as_str();
        // format!(
        //     "data:image-hash/svg+xml;base64,siphash128:{:x}",
        //     typst_shim::utils::hash128(hash)
        // )
        "data:image-hash/svg+xml;base64,redacted"
    });

    res.into()
}

#[test]
fn test_converted() {
    insta::assert_snapshot!(conv(r###"
= Hello, World!
This is a typst document.
        "###), @r###"
        # Hello, World!
        This is a typst document.
        "###);
    insta::assert_snapshot!(conv(r###"
Some inlined raw `a`, ```c b```
        "###), @"Some inlined raw `a`, `b`");
    insta::assert_snapshot!(conv(r###"
- Some *item*
- Another _item_
        "###), @r###"
        - Some **item**
        - Another _item_
        "###);
    insta::assert_snapshot!(conv(r###"
+ A
+ B
        "###), @r###"
        1. A
        1. B
        "###);
    insta::assert_snapshot!(conv(r###"
2. A
+ B
        "###), @r###"
        2. A
        1. B
        "###);
    insta::assert_snapshot!(conv(r###"
$
1/2 + 1/3 = 5/6
$
        "###), @r###"<p align="center"><img src="data:image-hash/svg+xml;base64,redacted" alt="typst-block" /></p>"###);
}
