use super::*;

#[test]
fn test_simple() {
    insta::assert_snapshot!(conv(r###"
https://example.com
        "###), @"[https://example.com](https://example.com)");
    insta::assert_snapshot!(conv(r###"
#link("https://example.com")[Content]
            "###), @"[Content](https://example.com)");
}

#[test]
fn test_nested() {
    insta::assert_snapshot!(conv(r###"
#link("https://example.com")[Reverse *the World*]
            "###), @"[Reverse **the World**](https://example.com)");
}
