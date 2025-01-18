use core::fmt;
use std::{num::NonZeroUsize, ops::RangeInclusive, str::FromStr};

use anyhow::Result;
use clap::ValueEnum;

macro_rules! display_possible_values {
    ($ty:ty) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.to_possible_value()
                    .expect("no values are skipped")
                    .get_name()
                    .fmt(f)
            }
        }
    };
}

/// When to export an output file.
#[derive(
    Debug,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
    ValueEnum,
)]
#[serde(rename_all = "camelCase")]
#[clap(rename_all = "camelCase")]
pub enum TaskWhen {
    /// Never watch to run task.
    Never,
    /// Run task on save.
    OnSave,
    /// Run task on type.
    OnType,
    /// *DEPRECATED* Run task when a document has a title and on saved, which is
    /// useful to filter out template files.
    ///
    /// Note: this is deprecating.
    OnDocumentHasTitle,
}

impl TaskWhen {
    /// Returns `true` if the task should never be run automatically.
    pub fn is_never(&self) -> bool {
        matches!(self, TaskWhen::Never)
    }
}

display_possible_values!(TaskWhen);

/// Which format to use for the generated output file.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, ValueEnum)]
pub enum OutputFormat {
    /// Export to PDF.
    Pdf,
    /// Export to PNG.
    Png,
    /// Export to SVG.
    Svg,
    /// Export to HTML.
    Html,
}

display_possible_values!(OutputFormat);

/// A PDF standard that Typst can enforce conformance with.
#[derive(Debug, Copy, Clone, Eq, PartialEq, ValueEnum, serde::Serialize, serde::Deserialize)]
#[allow(non_camel_case_types)]
pub enum PdfStandard {
    /// PDF 1.7.
    #[value(name = "1.7")]
    #[serde(rename = "1.7")]
    V_1_7,
    /// PDF/A-2b.
    #[value(name = "a-2b")]
    #[serde(rename = "a-2b")]
    A_2b,
}

display_possible_values!(PdfStandard);

/// Implements parsing of page ranges (`1-3`, `4`, `5-`, `-2`), used by the
/// `CompileCommand.pages` argument, through the `FromStr` trait instead of a
/// value parser, in order to generate better errors.
///
/// See also: <https://github.com/clap-rs/clap/issues/5065>
#[derive(Debug, Clone)]
pub struct Pages(pub RangeInclusive<Option<NonZeroUsize>>);

impl FromStr for Pages {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value
            .split('-')
            .map(str::trim)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [] | [""] => Err("page export range must not be empty"),
            [single_page] => {
                let page_number = parse_page_number(single_page)?;
                Ok(Pages(Some(page_number)..=Some(page_number)))
            }
            ["", ""] => Err("page export range must have start or end"),
            [start, ""] => Ok(Pages(Some(parse_page_number(start)?)..=None)),
            ["", end] => Ok(Pages(None..=Some(parse_page_number(end)?))),
            [start, end] => {
                let start = parse_page_number(start)?;
                let end = parse_page_number(end)?;
                if start > end {
                    Err("page export range must end at a page after the start")
                } else {
                    Ok(Pages(Some(start)..=Some(end)))
                }
            }
            [_, _, _, ..] => Err("page export range must have a single hyphen"),
        }
    }
}

impl fmt::Display for Pages {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = match self.0.start() {
            Some(start) => start.to_string(),
            None => String::from(""),
        };
        let end = match self.0.end() {
            Some(end) => end.to_string(),
            None => String::from(""),
        };
        write!(f, "{start}-{end}")
    }
}

impl serde::Serialize for Pages {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Pages {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// Parses a single page number.
fn parse_page_number(value: &str) -> Result<NonZeroUsize, &'static str> {
    if value == "0" {
        Err("page numbers start at one")
    } else {
        NonZeroUsize::from_str(value).map_err(|_| "not a valid page number")
    }
}
