//! Logging Functionality

use serde::{Deserialize, Serialize};

use super::*;

/// Log target for preview address announcements that external tools parse.
pub const PREVIEW_COMPAT_LOG_TARGET: &str = "tinymist::compat::preview";

/// Options for initializing the logger.
pub struct InitLogOpts {
    /// Whether the command should use verbose logging.
    pub verbose: bool,
    /// Additional filters to pass directly to `env_logger`.
    pub filter: Option<String>,
    /// Redirects output via LSP/DAP notification.
    pub output: Option<LspClient>,
}

/// Initializes the logger for the Tinymist library.
pub fn init_log(
    InitLogOpts {
        verbose,
        filter,
        output,
    }: InitLogOpts,
) -> anyhow::Result<()> {
    use log::LevelFilter::*;

    let base_level = if verbose { Info } else { Warn };

    let mut builder = env_logger::builder();
    if let Some(output) = output {
        builder.target(LogNotification::create(output));
    }

    // In WebAssembly, we use a custom notification for logging.
    #[cfg(target_arch = "wasm32")]
    {
        builder.format(|f, record| {
            use std::io::Write;
            let ts = tinymist_std::time::utc_now();

            write!(f, "[")?;
            ts.format_into(f, &tinymist_std::time::Rfc3339)
                .map_err(std::io::Error::other)?;
            writeln!(
                f,
                " {level:<5} {module_path} {file_path}:{line} {target}] {args}",
                level = record.level(),
                module_path = record.module_path().unwrap_or("unknown"),
                file_path = record.file().unwrap_or("unknown"),
                line = record.line().unwrap_or(0),
                target = record.target(),
                args = record.args()
            )
        });
    }

    configure_log_filters(&mut builder, base_level, filter.as_deref());

    Ok(builder.try_init()?)
}

fn configure_log_filters(
    builder: &mut env_logger::Builder,
    base_level: log::LevelFilter,
    filter: Option<&str>,
) {
    use log::LevelFilter::*;

    builder
        .filter_module("tinymist", base_level)
        .filter_module("tinymist_preview", base_level)
        .filter_module("typlite", base_level)
        .filter_module("reflexo", base_level)
        .filter_module("sync_ls", base_level)
        .filter_module("reflexo_typst::diag::console", base_level)
        // typst-preview.nvim and similar tools discover preview URLs by parsing
        // these INFO lines. Keep only this narrow compatibility target visible
        // in non-verbose CLI mode.
        .filter_module(PREVIEW_COMPAT_LOG_TARGET, Info);

    if let Some(f) = filter {
        builder.parse_filters(f);
    }
}

struct LogNotification(LspClient, Vec<u8>);

impl LogNotification {
    /// Creates a new `LogNotification` with the given LSP client and an empty
    /// buffer.
    fn create(output: LspClient) -> env_logger::Target {
        env_logger::Target::Pipe(Box::new(Self(output, vec![])))
    }
}

impl std::io::Write for LogNotification {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        self.1.extend_from_slice(buf);
        Ok(buf.len())
    }

    // todo: the from_utf8_lossy may break non-ascii characters and inefficient
    fn flush(&mut self) -> std::io::Result<()> {
        let data = String::from_utf8_lossy(self.1.as_slice()).to_string();
        self.1.clear();
        self.0.send_notification::<Log>(&Log { data });
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Log {
    data: String,
}

impl lsp_types::notification::Notification for Log {
    const METHOD: &'static str = "tmLog";
    type Params = Self;
}

#[cfg(test)]
mod tests {
    use log::{Level, Log, Metadata};

    use super::*;

    fn test_logger(verbose: bool, filter: Option<&str>) -> env_logger::Logger {
        use log::LevelFilter::*;

        let base_level = if verbose { Info } else { Warn };
        let mut builder = env_logger::builder();
        configure_log_filters(&mut builder, base_level, filter);
        builder.build()
    }

    fn enabled(logger: &env_logger::Logger, target: &str, level: Level) -> bool {
        logger.enabled(&Metadata::builder().target(target).level(level).build())
    }

    #[test]
    fn non_verbose_keeps_only_preview_address_info_enabled() {
        let logger = test_logger(false, None);

        assert!(enabled(&logger, PREVIEW_COMPAT_LOG_TARGET, Level::Info));
        assert!(!enabled(&logger, "tinymist::tool::preview", Level::Info));
        assert!(enabled(&logger, "tinymist::tool::preview", Level::Warn));
    }

    #[test]
    fn explicit_filter_can_override_preview_address_info() {
        let filter = format!("{PREVIEW_COMPAT_LOG_TARGET}=warn");
        let logger = test_logger(false, Some(&filter));

        assert!(!enabled(&logger, PREVIEW_COMPAT_LOG_TARGET, Level::Info));
        assert!(enabled(&logger, PREVIEW_COMPAT_LOG_TARGET, Level::Warn));
    }
}
