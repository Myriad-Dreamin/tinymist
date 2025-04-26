//! Attributes for HTML elements and parsing

use ecow::EcoString;
use tinymist_derive::TypliteAttr;
use typst::html::HtmlAttrs;

use crate::Result;

/// Tag attributes defined for HTML elements.
pub mod md_attr {
    use typst::html::HtmlAttr;

    macro_rules! attrs {
        ($($attr:ident -> $name:ident)*) => {
            $(#[allow(non_upper_case_globals)]
            pub const $attr: HtmlAttr = HtmlAttr::constant(
                stringify!($name)
            );)*
        }
    }

    attrs! {
        src -> src
        alt -> alt
        level -> level
        dest -> dest
        lang -> lang
        block -> block
        text -> text
        value -> value
    }
}

#[derive(TypliteAttr, Default)]
pub struct HeadingAttr {
    pub level: usize,
}

#[derive(TypliteAttr, Default)]
pub struct ImageAttr {
    pub src: EcoString,
    pub alt: EcoString,
}

#[derive(TypliteAttr, Default)]
pub struct LinkAttr {
    pub dest: EcoString,
}

#[derive(TypliteAttr, Default)]
pub struct RawAttr {
    pub lang: EcoString,
    pub block: bool,
    pub text: EcoString,
}

#[derive(TypliteAttr, Default)]
pub struct ListItemAttr {
    pub value: Option<u32>,
}

pub trait TypliteAttrsParser {
    fn parse(attrs: &HtmlAttrs) -> Result<Self>
    where
        Self: Sized;
}

pub trait TypliteAttrParser {
    fn parse_attr(content: &EcoString) -> Result<Self>
    where
        Self: Sized;
}

impl TypliteAttrParser for usize {
    fn parse_attr(content: &EcoString) -> Result<Self> {
        Ok(content
            .parse::<usize>()
            .map_err(|_| format!("cannot parse {} as usize", content))?)
    }
}

impl TypliteAttrParser for u32 {
    fn parse_attr(content: &EcoString) -> Result<Self> {
        Ok(content
            .parse::<u32>()
            .map_err(|_| format!("cannot parse {} as u32", content))?)
    }
}

impl TypliteAttrParser for bool {
    fn parse_attr(content: &EcoString) -> Result<Self> {
        Ok(content
            .parse::<bool>()
            .map_err(|_| format!("cannot parse {} as bool", content))?)
    }
}

impl TypliteAttrParser for EcoString {
    fn parse_attr(content: &EcoString) -> Result<Self> {
        Ok(content.clone())
    }
}

impl<T> TypliteAttrParser for Option<T>
where
    T: TypliteAttrParser,
{
    fn parse_attr(content: &EcoString) -> Result<Self> {
        if content.is_empty() {
            Ok(None)
        } else {
            T::parse_attr(content).map(Some)
        }
    }
}
