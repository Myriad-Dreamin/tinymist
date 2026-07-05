use tinymist::world::system::print_diagnostics;
use tinymist::world::{DiagnosticFormat, SourceWorld};
use tinymist_std::{bail, error::prelude::*};

pub fn exit_on_ctrl_c() {
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        log::info!("Ctrl-C received, exiting");
        std::process::exit(0);
    });
}

pub fn block_on<F: Future>(future: F) -> F::Output {
    crate::RUNTIMES.tokio_runtime.block_on(future)
}

pub fn print_diag_or_error<T>(world: &impl SourceWorld, result: Result<T>) -> Result<T> {
    match result {
        Ok(v) => Ok(v),
        Err(err) => {
            if let Some(diagnostics) = err.diagnostics() {
                print_diagnostics(world, diagnostics.iter(), DiagnosticFormat::Human)
                    .context_ut("print diagnostics")?;
                bail!("");
            }

            Err(err)
        }
    }
}
