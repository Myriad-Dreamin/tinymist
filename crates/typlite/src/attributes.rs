//! Attributes for HTML elements and parsing

use ecow::EcoString;
use tinymist_derive::TypliteAttr;
use typst_html::HtmlAttrs;

use crate::Result;

/// Tag attributes defined for HTML elements.
pub mod md_attr {
    use typst_html::HtmlAttr;

    macro_rules! attrs {
        ($($attr:ident -> $name:ident)*) => {
            $(#[allow(non_upper_case_globals)]
            pub const $attr: HtmlAttr = HtmlAttr::constant(
                stringify!($name)
            );)*
        }
    }

    attrs! {
        media -> media
        src -> src
        alt -> alt
        level -> level
        dest -> dest
        lang -> lang
        block -> block
        text -> text
        mode -> mode
        value -> value
        caption -> caption
        class -> class
        id -> id
    }
}

#[derive(TypliteAttr, Default)]
pub struct IdocAttr {
    pub src: EcoString,
    pub mode: EcoString,
}

#[derive(TypliteAttr, Default)]
pub struct HeadingAttr {
    pub id: EcoString,
    pub level: usize,
}

#[derive(TypliteAttr, Default)]
pub struct ImageAttr {
    pub id: EcoString,
    pub src: EcoString,
    pub alt: EcoString,
}

#[derive(TypliteAttr, Default)]
pub struct FigureAttr {
    pub id: EcoString,
    pub caption: EcoString,
}

#[derive(TypliteAttr, Default)]
pub struct LinkAttr {
    pub dest: EcoString,
}

#[derive(TypliteAttr, Default)]
pub struct RawAttr {
    pub id: EcoString,
    pub lang: EcoString,
    pub block: bool,
    pub text: EcoString,
}

#[derive(TypliteAttr, Default)]
pub struct ListItemAttr {
    pub value: Option<u32>,
}

#[derive(TypliteAttr, Default)]
pub struct AlertsAttr {
    pub class: EcoString,
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
            .map_err(|_| format!("cannot parse {content} as usize"))?)
    }
}

impl TypliteAttrParser for u32 {
    fn parse_attr(content: &EcoString) -> Result<Self> {
        Ok(content
            .parse::<u32>()
            .map_err(|_| format!("cannot parse {content} as u32"))?)
    }
}

impl TypliteAttrParser for bool {
    fn parse_attr(content: &EcoString) -> Result<Self> {
        Ok(content
            .parse::<bool>()
            .map_err(|_| format!("cannot parse {content} as bool"))?)
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
