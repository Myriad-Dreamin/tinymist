//! Custom HTML tags used by Typlite

/// Tag definitions specific to markdown conversion
pub mod md_tag {
    use typst::html::HtmlTag;

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
        linebreak -> m1linebreak
        image -> m1image
        strong -> m1strong
        emph -> m1emph
        highlight -> m1highlight
        strike -> m1strike
        raw -> m1raw
        label -> m1label
        reference -> m1ref
        heading -> m1heading
        outline -> m1outline
        outline_entry -> m1outentry
        quote -> m1quote
        table -> m1table
        // table_cell -> m1tablecell
        grid -> m1grid
        // grid_cell -> m1gridcell
        figure -> m1figure

        math_equation_inline -> m1eqinline
        math_equation_block -> m1eqblock

        doc -> m1document
        link -> m1link
    }
}
