use crate::tests::*;

#[test]
fn test_math_equation() {
    insta::assert_snapshot!(conv(r###"
$integral x dif x$
        "###), @r###"<img style="vertical-align: -0.35em" src="data:image-hash/svg+xml;base64,redacted" alt="typst-block" />"###);
    insta::assert_snapshot!(conv(r###"
$ integral x dif x $
        "###), @r###"<p align="center"><img src="data:image-hash/svg+xml;base64,redacted" alt="typst-block" /></p>"###);
}
