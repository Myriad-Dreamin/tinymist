mod model;
mod rendering;

use std::sync::OnceLock;

use regex::Regex;

use super::*;

fn conv(s: &str) -> EcoString {
    let res = Typlite::new_with_content(s.trim()).convert().unwrap();
    static REG: OnceLock<Regex> = OnceLock::new();
    let reg = REG.get_or_init(|| Regex::new(r#"data:image/svg\+xml;base64,([^"]+)"#).unwrap());
    let res = reg.replace(&res, |_captures: &regex::Captures| {
        // let hash = _captures.get(1).unwrap().as_str();
        // format!(
        //     "data:image-hash/svg+xml;base64,siphash128:{:x}",
        //     typst::util::hash128(hash)
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
    #[cfg(not(feature = "texmath"))]
    insta::assert_snapshot!(conv(r###"
$
1/2 + 1/3 = 5/6
$
        "###), @r###"


    <p align="center"><img src="data:image-hash/svg+xml;base64,siphash128:24036839c7ffd897ec5442095323ab4c" alt="typst-block" /></p>

    "###);
}
