//! Fully parallelized l10n tool for Rust and TypeScript.

use std::path::Path;

use clap::Parser;
use rayon::{
    iter::{ParallelBridge, ParallelIterator},
    str::ParallelString,
};
use tinymist_l10n::update_disk_translations;

/// The CLI arguments of the tool.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
struct Args {
    /// The kind of file to process.
    ///
    /// It can be `rs` for Rust or `ts` for TypeScript.
    /// - `rs`: checks `tinymist_l10n::t!` macro in Rust files.
    /// - `ts`: checks `l10nMsg` function in TypeScript files.
    #[clap(long)]
    kind: String,
    /// The directory to process recursively.
    #[clap(long)]
    dir: String,
    /// The output file to write the translations. The file will be in-place
    /// updated with new translations.
    #[clap(long)]
    output: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let is_rs = args.kind == "rs";
    let file_calls = walkdir::WalkDir::new(&args.dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|e| e == args.kind.as_str())
        })
        .par_bridge()
        .flat_map(|e| check_calls(e, is_rs))
        .collect::<Vec<_>>();

    update_disk_translations(file_calls, Path::new(&args.output))?;

    Ok(())
}

const L10N_FN_TS: &str = "l10nMsg";
const L10N_FN_RS: &str = "tinymist_l10n::t!";

fn check_calls(e: walkdir::DirEntry, is_rs: bool) -> Vec<(String, String)> {
    let path = e.path();
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!("failed to read file {path:?}: {err}");
            return Vec::new();
        }
    };

    content
        .as_str()
        .par_match_indices(if is_rs { 't' } else { 'l' })
        .flat_map(|e| {
            let s = &content[e.0..];
            if !is_rs && s.starts_with(L10N_FN_TS) {
                let suffix = &content[e.0 + L10N_FN_TS.len()..];
                return parse_l10n_args_ts(suffix);

                fn parse_l10n_args_ts(s: &str) -> Option<(String, String)> {
                    let s = parse_char(s, '(')?;
                    let (key, _s) = parse_str(s)?;
                    Some((format!("\"{key}\""), format!("\"{key}\"")))
                }
            }
            if is_rs && s.starts_with(L10N_FN_RS) {
                let suffix = &content[e.0 + L10N_FN_RS.len()..];
                return parse_l10n_args_rs(suffix);

                fn parse_l10n_args_rs(s: &str) -> Option<(String, String)> {
                    let s = parse_char(s, '(')?;
                    let (key, s) = parse_str(s)?;
                    let s = parse_char(s, ',')?;
                    let (value, _s) = parse_str(s)?;
                    Some((key.to_string(), format!("\"{value}\"")))
                }
            }
            None
        })
        .collect::<Vec<_>>()
}

fn parse_char(s: &str, ch: char) -> Option<&str> {
    let s = s.trim_start();
    if s.starts_with(ch) {
        Some(&s[1..])
    } else {
        None
    }
}

fn parse_str(s: &str) -> Option<(&str, &str)> {
    let s = parse_char(s, '"')?;

    let mut escape = false;

    for (i, ch) in s.char_indices() {
        if escape {
            escape = false;
        } else {
            match ch {
                '\\' => escape = true,
                '"' => return Some((&s[..i], &s[i + 1..])),
                _ => (),
            }
        }
    }

    None
}
