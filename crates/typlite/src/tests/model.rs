use super::*;

#[test]
fn test_simple_link() {
    insta::assert_snapshot!(conv(r###"
https://example.com
        "###), @"[https://example.com](https://example.com)");
    insta::assert_snapshot!(conv(r###"
#link("https://example.com")[Content]
            "###), @"[Content](https://example.com)");
}

#[test]
fn test_nested_link() {
    insta::assert_snapshot!(conv(r###"
#link("https://example.com")[Reverse *the World*]
            "###), @"[Reverse **the World**](https://example.com)");
}

#[test]
fn test_simple_image() {
    insta::assert_snapshot!(conv(r###"
#image("./fig.png")
            "###), @"![](./fig.png)");
    insta::assert_snapshot!(conv(r###"
#image("./fig.png", alt: "Content")
            "###), @"![Content](./fig.png)");
}

#[test]
fn test_simple_figure() {
    insta::assert_snapshot!(conv(r###"
#figure(image("./fig.png"))
            "###), @"![](./fig.png)");
    insta::assert_snapshot!(conv(r###"
#figure(image("./fig.png", alt: "Content"))
            "###), @"![Content](./fig.png)");
    insta::assert_snapshot!(conv(r###"
#figure(image("./fig.png", alt: "Content"), caption: "Caption")
            "###), @"![Caption, Content](./fig.png)");
}
