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
        source -> source
        width -> width
        height -> height
        level -> level
        dest -> dest
        kind -> kind
        lang -> lang
        block -> block
        text -> text
        mode -> mode
        value -> value
        caption -> caption
        class -> class
        id -> id
        tight -> tight
        reversed -> reversed
        start -> start
        columns -> columns
        numbering -> numbering
        target -> target
        supplement -> supplement
        key -> key
        full -> full
        title -> title
        style -> style
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
    pub numbering: EcoString,
}

#[derive(TypliteAttr, Default)]
pub struct ImageAttr {
    pub id: EcoString,
    pub source: EcoString,
    pub alt: EcoString,
    pub width: EcoString,
    pub height: EcoString,
}

#[derive(TypliteAttr, Default)]
pub struct FigureAttr {
    pub id: EcoString,
    pub kind: EcoString,
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
pub struct EquationAttr {
    pub block: bool,
}

#[derive(TypliteAttr, Default)]
pub struct ListItemAttr {
    pub value: Option<u32>,
}

#[derive(TypliteAttr, Default)]
pub struct ListAttr {
    pub tight: bool,
}

#[derive(TypliteAttr, Default)]
pub struct EnumAttr {
    pub tight: bool,
    pub start: Option<u32>,
    pub reversed: bool,
}

#[derive(TypliteAttr, Default)]
pub struct TermsAttr {
    pub tight: bool,
}

#[derive(TypliteAttr, Default)]
pub struct AlertsAttr {
    pub class: EcoString,
}

#[derive(TypliteAttr, Default)]
pub struct TableAttr {
    pub columns: Option<usize>,
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
        if content.is_empty() {
            return Ok(false);
        }

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
        if content.is_empty() || content.as_str() == "auto" {
            Ok(None)
        } else {
            T::parse_attr(content).map(Some)
        }
    }
}
