//! Logging Functionality

use serde::{Deserialize, Serialize};

use super::*;

/// Options for initializing the logger.
pub struct InitLogOpts {
    /// Whether the command is transient (e.g., compile).   
    pub is_transient_cmd: bool,
    /// Whether the command is a test without verbose output.
    pub is_test_no_verbose: bool,
    /// Redirects output via LSP/DAP notification.
    pub output: Option<LspClient>,
}

/// Initializes the logger for the Tinymist library.
pub fn init_log(
    InitLogOpts {
        is_transient_cmd,
        is_test_no_verbose,
        output,
    }: InitLogOpts,
) -> anyhow::Result<()> {
    use log::LevelFilter::*;

    let base_no_info = is_transient_cmd || is_test_no_verbose;
    let base_level = if base_no_info { Warn } else { Info };
    let preview_level = if is_test_no_verbose { Warn } else { Info };
    let diag_level = if is_test_no_verbose { Warn } else { Info };

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

    Ok(builder
        .filter_module("tinymist", base_level)
        .filter_module("tinymist_preview", preview_level)
        .filter_module("typlite", base_level)
        .filter_module("reflexo", base_level)
        .filter_module("sync_ls", base_level)
        .filter_module("reflexo_typst2vec::pass::span2vec", Error)
        .filter_module("reflexo_typst::diag::console", diag_level)
        .try_init()?)
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
