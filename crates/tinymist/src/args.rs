use once_cell::sync::Lazy;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
#[cfg_attr(feature = "clap", clap(name = "tinymist", author, version, about, long_version(LONG_VERSION.as_str())))]
pub struct CliArguments {
    /// Mirror the stdin to the specified file
    #[cfg_attr(feature = "clap", clap(long, default_value = "", value_name = "FILE"))]
    pub mirror: String,
    /// Mirror input from the file
    #[cfg_attr(feature = "clap", clap(long, default_value = "", value_name = "FILE"))]
    pub mirror_input: String,
}

pub static LONG_VERSION: Lazy<String> = Lazy::new(|| {
    format!(
        "
Build Timestamp:     {}
Build Git Describe:  {}
Commit SHA:          {}
Commit Date:         {}
Commit Branch:       {}
Cargo Target Triple: {}
Typst Version:       {}
",
        env!("VERGEN_BUILD_TIMESTAMP"),
        env!("VERGEN_GIT_DESCRIBE"),
        option_env!("VERGEN_GIT_SHA").unwrap_or("None"),
        option_env!("VERGEN_GIT_COMMIT_TIMESTAMP").unwrap_or("None"),
        option_env!("VERGEN_GIT_BRANCH").unwrap_or("None"),
        env!("VERGEN_CARGO_TARGET_TRIPLE"),
        env!("TYPST_VERSION"),
    )
});
