//! Custom HTML tags used by Typlite

/// Tag definitions specific to markdown conversion
pub mod md_tag {
    use typst_html::HtmlTag;

    macro_rules! tags {
        ($($tag:ident -> $name:ident)*) => {
            $(#[allow(non_upper_case_globals)]
            pub const $tag: HtmlTag = HtmlTag::constant(
                stringify!($name)
            );)*
        }
    }

    tags! {
        parbreak -> m1parbreak
        verbatim -> m1verbatim
        idoc -> m1idoc
        source -> m1source
        grid -> m1grid

        math_equation_inline -> m1eqinline
        math_equation_block -> m1eqblock
        alerts -> m1alerts
        doc -> m1document
    }
}
