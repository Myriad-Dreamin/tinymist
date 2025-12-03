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
        linebreak -> m1linebreak
        pagebreak -> m1pagebreak
        image -> m1image
        strong -> m1strong
        emph -> m1emph
        underline -> m1underline
        strike -> m1strike
        overline -> m1overline
        sub -> m1sub
        superscript -> m1super
        highlight -> m1highlight
        smallcaps -> m1smallcaps
        footnote -> m1footnote
        raw -> m1raw
        verbatim -> m1verbatim
        label -> m1label
        reference -> m1ref
        cite -> m1cite
        sources -> m1sources
        heading -> m1heading
        outline -> m1outline
        outline_entry -> m1outentry
        quote -> m1quote
        table -> m1table
        cell -> m1cell
        header -> m1header
        footer -> m1footer
        idoc -> m1idoc
        source -> m1source
        grid -> m1grid
        figure -> m1figure
        figure_body -> m1body
        figure_caption -> m1caption

        math_equation_inline -> m1eqinline
        math_equation_block -> m1eqblock
        equation -> m1equation
        alerts -> m1alerts
        doc -> m1document
        link -> m1link
        link_dest -> m1dest
        link_body -> m1body
        list -> m1list
        r#enum -> m1enum
        terms -> m1terms
        item -> m1item
        term_entry -> m1term
        body -> m1body
    }
}
