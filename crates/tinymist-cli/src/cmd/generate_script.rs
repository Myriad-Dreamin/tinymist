//! Project management tools.

use std::{borrow::Cow, path::Path};

use reflexo::path::unix_slash;
use tinymist::project::*;
use tinymist_std::{bail, error::prelude::*};

/// Arguments for generating a build script.
#[derive(Debug, Clone, clap::Parser)]
pub struct GenerateScriptArgs {
    /// The shell to generate the completion script for. If not provided, it
    /// will be inferred from the environment.
    #[clap(value_enum)]
    pub shell: Option<Shell>,
    /// The path to the output script.
    #[clap(short, long)]
    pub output: Option<String>,
}

/// Generates a build script for compilation
pub fn generate_script_main(args: GenerateScriptArgs) -> Result<()> {
    let Some(shell) = args.shell.or_else(Shell::from_env) else {
        bail!("could not infer shell");
    };
    let output = Path::new(args.output.as_deref().unwrap_or("build"));

    let output = match shell {
        Shell::Bash | Shell::Zsh | Shell::Elvish | Shell::Fish => output.with_extension("sh"),
        Shell::PowerShell => output.with_extension("ps1"),
        _ => bail!("unsupported shell: {shell:?}"),
    };

    let script = match shell {
        Shell::Bash | Shell::Zsh | Shell::PowerShell => shell_build_script(shell)?,
        _ => bail!("unsupported shell: {shell:?}"),
    };

    std::fs::write(output, script).context("write script")?;

    Ok(())
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, clap::ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum Shell {
    Bash,
    Elvish,
    Fig,
    Fish,
    PowerShell,
    Zsh,
    Nushell,
}

impl Shell {
    pub fn from_env() -> Option<Self> {
        if let Some(env_shell) = std::env::var_os("SHELL") {
            let name = Path::new(&env_shell).file_stem()?.to_str()?;

            match name {
                "bash" => Some(Shell::Bash),
                "zsh" => Some(Shell::Zsh),
                "fig" => Some(Shell::Fig),
                "fish" => Some(Shell::Fish),
                "elvish" => Some(Shell::Elvish),
                "powershell" | "powershell_ise" => Some(Shell::PowerShell),
                "nushell" => Some(Shell::Nushell),
                _ => None,
            }
        } else if cfg!(windows) {
            Some(Shell::PowerShell)
        } else {
            None
        }
    }
}

impl clap_complete::Generator for Shell {
    fn file_name(&self, name: &str) -> String {
        use clap_complete::shells::{Bash, Elvish, Fish, PowerShell, Zsh};
        use clap_complete_fig::Fig;
        use clap_complete_nushell::Nushell;

        match self {
            Shell::Bash => Bash.file_name(name),
            Shell::Elvish => Elvish.file_name(name),
            Shell::Fig => Fig.file_name(name),
            Shell::Fish => Fish.file_name(name),
            Shell::PowerShell => PowerShell.file_name(name),
            Shell::Zsh => Zsh.file_name(name),
            Shell::Nushell => Nushell.file_name(name),
        }
    }

    fn generate(&self, cmd: &clap::Command, buf: &mut dyn std::io::Write) {
        use clap_complete::shells::{Bash, Elvish, Fish, PowerShell, Zsh};
        use clap_complete_fig::Fig;
        use clap_complete_nushell::Nushell;

        match self {
            Shell::Bash => Bash.generate(cmd, buf),
            Shell::Elvish => Elvish.generate(cmd, buf),
            Shell::Fig => Fig.generate(cmd, buf),
            Shell::Fish => Fish.generate(cmd, buf),
            Shell::PowerShell => PowerShell.generate(cmd, buf),
            Shell::Zsh => Zsh.generate(cmd, buf),
            Shell::Nushell => Nushell.generate(cmd, buf),
        }
    }
}

/// Generates a build script for shell-like shells
fn shell_build_script(shell: Shell) -> Result<String> {
    let mut output = String::new();

    match shell {
        Shell::Bash => {
            output.push_str("#!/usr/bin/env bash\n\n");
        }
        Shell::Zsh => {
            output.push_str("#!/usr/bin/env zsh\n\n");
        }
        Shell::PowerShell => {}
        _ => {}
    }

    let lock_dir = std::env::current_dir().context("current directory")?;

    let lock = LockFile::read(&lock_dir)?;

    struct CmdBuilder(Vec<Cow<'static, str>>);

    impl CmdBuilder {
        fn new() -> Self {
            Self(vec![])
        }

        fn extend(&mut self, args: impl IntoIterator<Item = impl Into<Cow<'static, str>>>) {
            for arg in args {
                self.0.push(arg.into());
            }
        }

        fn push(&mut self, arg: impl Into<Cow<'static, str>>) {
            self.0.push(arg.into());
        }

        fn build(self) -> String {
            self.0.join(" ")
        }
    }

    let quote_escape = |s: &str| s.replace("'", r#"'"'"'"#);
    let quote = |s: &str| format!("'{}'", s.replace("'", r#"'"'"'"#));

    let path_of = |p: &ResourcePath, loc: &str| {
        let Some(path) = p.to_rel_path(&lock_dir) else {
            log::error!("could not resolve path for {loc}, path: {p:?}");
            return String::default();
        };

        quote(&unix_slash(&path))
    };

    let base_cmd: Vec<&str> = vec!["tinymist", "compile", "--save-lock"];

    for task in lock.task.iter() {
        let Some(input) = lock.get_document(&task.document) else {
            log::warn!(
                "could not find document for task {:?}, whose document is {:?}",
                task.id,
                task.doc_id()
            );
            continue;
        };
        // todo: preview/query commands
        let Some(export) = task.task.as_export() else {
            continue;
        };

        let mut cmd = CmdBuilder::new();
        cmd.extend(base_cmd.iter().copied());
        cmd.push("--task");
        cmd.push(quote(&task.id.to_string()));

        cmd.push(path_of(&input.main, "main"));

        if let Some(root) = &input.root {
            cmd.push("--root");
            cmd.push(path_of(root, "root"));
        }

        for (k, v) in &input.inputs {
            cmd.push(format!(
                r#"--input='{}={}'"#,
                quote_escape(k),
                quote_escape(v)
            ));
        }

        for p in &input.font_paths {
            cmd.push("--font-path");
            cmd.push(path_of(p, "font-path"));
        }

        if !input.system_fonts {
            cmd.push("--ignore-system-fonts");
        }

        if let Some(p) = &input.package_path {
            cmd.push("--package-path");
            cmd.push(path_of(p, "package-path"));
        }

        if let Some(p) = &input.package_cache_path {
            cmd.push("--package-cache-path");
            cmd.push(path_of(p, "package-cache-path"));
        }

        if let Some(p) = &export.output {
            cmd.push("--output");
            cmd.push(quote(&p.to_string()));
        }

        for t in &export.transform {
            match t {
                ExportTransform::Pretty { .. } => {
                    cmd.push("--pretty");
                }
                ExportTransform::Pages { ranges } => {
                    for r in ranges {
                        cmd.push("--pages");
                        cmd.push(r.to_string());
                    }
                }
                // todo: export me
                ExportTransform::Merge { .. } | ExportTransform::Script { .. } => {}
            }
        }

        match &task.task {
            ProjectTask::Preview(..) | ProjectTask::Query(..) => {}
            ProjectTask::ExportPdf(task) => {
                cmd.push("--format=pdf");

                for s in &task.pdf_standards {
                    cmd.push("--pdf-standard");
                    let s = serde_json::to_string(s).context("pdf standard")?;
                    cmd.push(s);
                }

                if let Some(output) = &task.creation_timestamp {
                    cmd.push("--creation-timestamp");
                    cmd.push(output.to_string());
                }
            }
            ProjectTask::ExportSvg(..) => {
                cmd.push("--format=svg");
            }
            ProjectTask::ExportSvgHtml(..) => {
                cmd.push("--format=svg_html");
            }
            ProjectTask::ExportMd(..) => {
                cmd.push("--format=md");
            }
            ProjectTask::ExportTeX(..) => {
                cmd.push("--format=tex");
            }
            ProjectTask::ExportPng(..) => {
                cmd.push("--format=png");
            }
            ProjectTask::ExportText(..) => {
                cmd.push("--format=txt");
            }
            ProjectTask::ExportHtml(..) => {
                cmd.push("--format=html");
            }
        }

        let ext = task.task.extension();

        output.push_str(&format!(
            "# From {} to {} ({ext})\n",
            task.doc_id(),
            task.id
        ));
        output.push_str(&cmd.build());
        output.push('\n');
    }

    Ok(output)
}
