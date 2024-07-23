mod model;

use super::*;

fn conv(s: &str) -> EcoString {
    Typlite::new_with_content(s.trim()).convert().unwrap()
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
    ```typ
    $

    $
    ```
    "###);
}
