use std::path::Path;

#[cfg(feature = "preview")]
use tinymist_project::LockFile;
use tinymist_std::error::prelude::*;
#[cfg(feature = "preview")]
use tinymist_task::Id;

/// Project document commands' main
#[cfg(feature = "lock")]
pub fn doc_main(args: tinymist_project::DocCommands) -> Result<()> {
    use tinymist_project::DocCommands;

    let cwd = std::env::current_dir().context("cannot get cwd")?;
    LockFile::update(&cwd, |state| {
        let ctx: (&Path, &Path) = (&cwd, &cwd);
        match args {
            DocCommands::New(args) => {
                state.replace_document(args.to_input(ctx));
            }
            DocCommands::Configure(args) => {
                use tinymist_project::ProjectRoute;

                let id: Id = args.id.id(ctx);

                state.route.push(ProjectRoute {
                    id: id.clone(),
                    priority: args.priority,
                });
            }
        }

        Ok(())
    })
}
