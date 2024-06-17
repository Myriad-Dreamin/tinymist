use std::path::PathBuf;

use once_cell::sync::Lazy;

// enum Preview Mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum PreviewMode {
    /// Preview mode for regular document
    #[cfg_attr(feature = "clap", clap(name = "document"))]
    Document,

    /// Preview mode for slide
    #[cfg_attr(feature = "clap", clap(name = "slide"))]
    Slide,
}

#[cfg(feature = "clap")]
const ENV_PATH_SEP: char = if cfg!(windows) { ';' } else { ':' };

#[derive(Debug, Clone)]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
pub struct PreviewArgs {
    /// Data plane server will bind to this address
    #[cfg_attr(
        feature = "clap",
        clap(
            long = "data-plane-host",
            default_value = "127.0.0.1:23625",
            value_name = "HOST",
            hide(true)
        )
    )]
    pub data_plane_host: String,

    /// Control plane server will bind to this address
    #[cfg_attr(
        feature = "clap",
        clap(
            long = "control-plane-host",
            default_value = "127.0.0.1:23626",
            value_name = "HOST",
            hide(true)
        )
    )]
    pub control_plane_host: String,

    /// Only render visible part of the document. This can improve performance
    /// but still being experimental.
    #[cfg_attr(feature = "clap", clap(long = "partial-rendering"))]
    pub enable_partial_rendering: bool,

    /// Invert colors of the preview (useful for dark themes without cost).
    /// Please note you could see the origin colors when you hover elements in
    /// the preview.
    #[clap(long, default_value = "never")]
    pub invert_colors: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
#[cfg_attr(feature = "clap", clap(name = "typst-preview", author, version, about, long_version(LONG_VERSION.as_str())))]
pub struct CliArguments {
    #[cfg_attr(feature = "clap", clap(flatten))]
    pub preview: PreviewArgs,

    /// Preview mode
    #[cfg_attr(
        feature = "clap",
        clap(long = "preview-mode", default_value = "document", value_name = "MODE")
    )]
    pub preview_mode: PreviewMode,

    /// Host for the preview server
    #[cfg_attr(
        feature = "clap",
        clap(
            long = "host",
            value_name = "HOST",
            default_value = "127.0.0.1:23627",
            alias = "static-file-host"
        )
    )]
    pub static_file_host: String,

    /// Don't open the preview in the browser after compilation.
    #[cfg_attr(feature = "clap", clap(long = "no-open"))]
    pub dont_open_in_browser: bool,

    /// Add a string key-value pair visible through `sys.inputs`
    #[clap(
        long = "input",
        value_name = "key=value",
        action = clap::ArgAction::Append,
        value_parser = clap::builder::ValueParser::new(parse_input_pair),
    )]
    pub inputs: Vec<(String, String)>,

    /// Ensures system fonts won't be searched, unless explicitly included via
    /// `--font-path`
    #[arg(long)]
    pub ignore_system_fonts: bool,

    /// Add additional directories to search for fonts
    #[cfg_attr(
        feature = "clap",
        clap(
            long = "font-path",
            value_name = "DIR",
            action = clap::ArgAction::Append,
            env = "TYPST_FONT_PATHS",
            value_delimiter = ENV_PATH_SEP,
        )
    )]
    pub font_paths: Vec<PathBuf>,

    /// Root directory for your project
    #[cfg_attr(feature = "clap", clap(long = "root", value_name = "DIR"))]
    pub root: Option<PathBuf>,

    pub input: PathBuf,
}

// This parse function comes from typst-cli
/// Parses key/value pairs split by the first equal sign.
///
/// This function will return an error if the argument contains no equals sign
/// or contains the key (before the equals sign) is empty.
fn parse_input_pair(raw: &str) -> Result<(String, String), String> {
    let (key, val) = raw
        .split_once('=')
        .ok_or("input must be a key and a value separated by an equal sign")?;
    let key = key.trim().to_owned();
    if key.is_empty() {
        return Err("the key was missing or empty".to_owned());
    }
    let val = val.trim().to_owned();
    Ok((key, val))
}

pub static LONG_VERSION: Lazy<String> = Lazy::new(|| {
    format!(
        "
Version:             {}
Build Timestamp:     {}
Build Git Describe:  {}
Commit SHA:          {}
Commit Date:         {}
Commit Branch:       {}
Cargo Target Triple: {}
",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_BUILD_TIMESTAMP"),
        env!("VERGEN_GIT_DESCRIBE"),
        option_env!("VERGEN_GIT_SHA").unwrap_or("None"),
        option_env!("VERGEN_GIT_COMMIT_TIMESTAMP").unwrap_or("None"),
        option_env!("VERGEN_GIT_BRANCH").unwrap_or("None"),
        env!("VERGEN_CARGO_TARGET_TRIPLE"),
    )
});
