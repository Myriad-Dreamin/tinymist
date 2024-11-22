use crate::tests::*;

#[test]
fn test_math_equation() {
    insta::assert_snapshot!(conv(r###"
$integral x dif x$
        "###), @r#"<picture><source media="(prefers-color-scheme: dark)" srcset="data:image-hash/svg+xml;base64,redacted"><img style="vertical-align: -0.35em" alt="typst-block" src="data:image-hash/svg+xml;base64,redacted" /></picture>"#);
    insta::assert_snapshot!(conv(r###"
$ integral x dif x $
        "###), @r#"<p align="center"><picture><source media="(prefers-color-scheme: dark)" srcset="data:image-hash/svg+xml;base64,redacted"><img alt="typst-block" src="data:image-hash/svg+xml;base64,redacted" /></picture></p>"#);
}
