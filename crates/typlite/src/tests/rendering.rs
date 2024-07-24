use crate::tests::*;

#[test]
fn test_math_equation() {
    insta::assert_snapshot!(conv(r###"
$integral x dif x$
        "###), @r###"<img style="vertical-align: -0.35em" src="data:image-hash/svg+xml;base64,siphash128:b15c9f3de28b301e676a2795b2423e78" alt="typst-block" />"###);
    insta::assert_snapshot!(conv(r###"
$ integral x dif x $
        "###), @r###"


    <p align="center"><img src="data:image-hash/svg+xml;base64,siphash128:5689ad380f9a6dca21e40b703231aca6" alt="typst-block" /></p>

    "###);
}
