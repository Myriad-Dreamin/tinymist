use super::*;

#[test]
fn test_simple() {
    insta::assert_snapshot!(conv(r###"
https://example.com
        "###), @"[https://example.com](https://example.com)");
    insta::assert_snapshot!(conv(r###"
#link("https://example.com")[Content]
            "###), @"");
}
