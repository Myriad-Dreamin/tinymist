use std::path::{Path, PathBuf};
use std::str::FromStr;

use futures::future::MaybeDone;
use reflexo_typst::package::PackageSpec;
use sync_ls::transport::{MirrorArgs, with_stdio_transport};
use sync_ls::{LspBuilder, LspMessage, LspResult, internal_error};
use tinymist::{Config, ServerState, SuperInit};
use tinymist_query::package::PackageInfo;
use tinymist_std::error::prelude::*;

use crate::*;

#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "camelCase")]
pub enum QueryCommands {
    /// Get the documentation for a specific package.
    PackageDocs(PackageDocsArgs),
    /// Check a specific package.
    CheckPackage(PackageDocsArgs),
}

#[derive(Debug, Clone, clap::Parser)]
pub struct PackageDocsArgs {
    /// The path of the package to request docs for.
    #[clap(long)]
    pub path: Option<String>,
    /// The package of the package to request docs for.
    #[clap(long)]
    pub id: String,
    /// The output path for the requested docs.
    #[clap(short, long)]
    pub output: String,
    // /// The format of requested docs.
    // #[clap(long)]
    // pub format: Option<QueryDocsFormat>,
}

/// The main entry point for language server queries.
pub fn query_main(cmds: QueryCommands) -> Result<()> {
    use tinymist_project::package::PackageRegistry;

    with_stdio_transport::<LspMessage>(MirrorArgs::default(), |conn| {
        let client_root = client_root(conn.sender);
        let client = client_root.weak();

        // todo: roots, inputs, font_opts
        let config = Config::default();

        let mut service = ServerState::install_lsp(LspBuilder::new(
            SuperInit {
                client: client.to_typed(),
                exec_cmds: Vec::new(),
                config,
                err: None,
            },
            client.clone(),
        ))
        .build();

        let resp = service.ready(()).unwrap();
        let MaybeDone::Done(resp) = resp else {
            anyhow::bail!("internal error: not sync init")
        };
        resp.unwrap();

        let state = service.state_mut().unwrap();

        let snap = state.snapshot().unwrap();
        let res = RUNTIMES.tokio_runtime.block_on(async move {
            match cmds {
                QueryCommands::PackageDocs(args) => {
                    let pkg = PackageSpec::from_str(&args.id).unwrap();
                    let path = args.path.map(PathBuf::from);
                    let path = path
                        .unwrap_or_else(|| snap.registry().resolve(&pkg).unwrap().as_ref().into());

                    let res = state
                        .resource_package_docs_(PackageInfo {
                            path,
                            namespace: pkg.namespace,
                            name: pkg.name,
                            version: pkg.version.to_string(),
                        })?
                        .await?;

                    let output_path = Path::new(&args.output);
                    std::fs::write(output_path, res).map_err(internal_error)?;
                }
                QueryCommands::CheckPackage(args) => {
                    let pkg = PackageSpec::from_str(&args.id).unwrap();
                    let path = args.path.map(PathBuf::from);
                    let path = path
                        .unwrap_or_else(|| snap.registry().resolve(&pkg).unwrap().as_ref().into());

                    state
                        .check_package(PackageInfo {
                            path,
                            namespace: pkg.namespace,
                            name: pkg.name,
                            version: pkg.version.to_string(),
                        })?
                        .await?;
                }
            };

            LspResult::Ok(())
        });

        res.map_err(|e| anyhow::anyhow!("{e:?}"))
    })?;

    Ok(())
}

#[derive(Debug, Clone, Default, clap::ValueEnum)]
#[clap(rename_all = "camelCase")]
enum QueryDocsFormat {
    #[default]
    Json,
    Markdown,
}
