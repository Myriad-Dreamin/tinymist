use std::{io, path::PathBuf, sync::LazyLock};

use tinymist_std::path::PathClean;

fn find_git_root() -> io::Result<PathBuf> {
    while !PathBuf::from(".git").exists() {
        if std::env::set_current_dir("..").is_err() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Git root not found",
            ));
        }
    }

    std::env::current_dir()
}

pub static GIT_ROOT: LazyLock<PathBuf> =
    LazyLock::new(|| find_git_root().expect("Failed to find git root").clean());

/// The CLI is the executable that already put inside of the VSCode extension's
/// `out` directory. This is intented, to ensure that we are testing the CLI to
/// publish in future.
pub static CLI: LazyLock<PathBuf> = LazyLock::new(|| {
    let cli = if cfg!(windows) {
        GIT_ROOT.join("editors/vscode/out/tinymist.exe")
    } else {
        GIT_ROOT.join("editors/vscode/out/tinymist")
    };

    if !cli.exists() {
        panic!("tinymist binary for e2e tests doesn't exist. Please ensure that tinymist binary to publish is ready on {cli:?}. Running scripts/e2e.{{sh/ps1}} should also help this.");
    }

    cli
});

pub fn cli() -> std::process::Command {
    let mut cmd = std::process::Command::new(&*CLI);
    cmd.current_dir(&*GIT_ROOT);
    cmd
}
