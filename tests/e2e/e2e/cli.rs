use tinymist_std::path::PathClean;

use crate::artifact::{cli, GIT_ROOT};

macro_rules! apply_common_filters {
    {} => {
        let mut settings = insta::Settings::clone_current();
        // Env redaction
        // [env: key=value]
        settings.add_filter(r"\[env:\s*([^=]+)=[^\]]*\]", "[env: $1=REDACTED]");
        settings.add_filter(r"tinymist.exe", "tinymist");
        let _bound = settings.bind_to_scope();
    }
}

#[test]
fn test_probe() {
    insta_cmd::assert_cmd_snapshot!(cli().arg("probe"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");
}

#[test]
fn test_help() {
    apply_common_filters!();
    insta_cmd::assert_cmd_snapshot!(cli().arg("--help"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    An integrated language service for Typst.

    Usage: tinymist [OPTIONS] [COMMAND]

    Commands:
      probe       Probe existence (Nop run)
      lsp         Run language server
      dap         Run debug adapter
      preview     Run preview server
      compile     Run compile command like `typst-cli compile`
      lint        Run Tinymist lint checks
      package     Run package tools
      completion  Generate completion script to stdout
      test        Test a document and give summary
      help        Print this message or the help of the given subcommand(s)

    Options:
          --log-filter <LOG_FILTER>  Configure log filter of tinymist [env: TINYMIST_LOG=REDACTED]
      -h, --help                     Print help
      -V, --version                  Print version

    ----- stderr -----
    ");
}

#[test]
fn test_help_lsp() {
    apply_common_filters!();
    insta_cmd::assert_cmd_snapshot!(cli().arg("lsp").arg("--help"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    Run language server

    Usage: tinymist lsp [OPTIONS]

    Options:
          --mirror <FILE>
              Mirror the stdin to the file
              
              [default: ""]

          --replay <FILE>
              Replay input from the file
              
              [default: ""]

          --font-path <DIR>
              Add additional directories that are recursively searched for fonts.
              
              If multiple paths are specified, they are separated by the system's path separator (`:` on
              Unix-like systems and `;` on Windows).
              
              [env: TYPST_FONT_PATHS=REDACTED]

          --ignore-system-fonts
              Ensure system fonts won't be searched, unless explicitly included via `--font-path`

      -h, --help
              Print help (see a summary with '-h')

    ----- stderr -----
    "#);
}

#[test]
fn test_help_compile_alias() {
    apply_common_filters!();
    insta_cmd::assert_cmd_snapshot!(cli().arg("c").arg("--help"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Run compile command like `typst-cli compile`

    Usage: tinymist compile [OPTIONS] <INPUT> [OUTPUT]

    Arguments:
      <INPUT>
              Specify the path to input Typst file

      [OUTPUT]
              Provide the path to output file (PDF, PNG, SVG, or HTML). Use `-` to write output to
              stdout.
              
              For output formats emitting one file per page (PNG & SVG), a page number template must be
              present if the source document renders to multiple pages. Use `{p}` for page numbers,
              `{0p}` for zero padded page numbers and `{t}` for page count. For example,
              `page-{0p}-of-{t}.png` creates `page-01-of-10.png`, `page-02-of-10.png`, and so on.

    Options:
          --name <NAME>
              Give a task name to the document

          --root <DIR>
              Configure the project root (for absolute paths). If the path is relative, it will be
              resolved relative to the current working directory (PWD)
              
              [env: TYPST_ROOT=REDACTED]

          --input <key=value>
              Add a string key-value pair visible through `sys.inputs`

          --font-path <DIR>
              Add additional directories that are recursively searched for fonts.
              
              If multiple paths are specified, they are separated by the system's path separator (`:` on
              Unix-like systems and `;` on Windows).
              
              [env: TYPST_FONT_PATHS=REDACTED]

          --ignore-system-fonts
              Ensure system fonts won't be searched, unless explicitly included via `--font-path`

          --package-path <DIR>
              Specify a custom path to local packages, defaults to system-dependent location
              
              [env: TYPST_PACKAGE_PATH=REDACTED]

          --package-cache-path <DIR>
              Specify a custom path to package cache, defaults to system-dependent location
              
              [env: TYPST_PACKAGE_CACHE_PATH=REDACTED]

          --when <WHEN>
              Configure when to run the task

              Possible values:
              - never:              Never watch to run task
              - onSave:             Run task on saving the document, i.e. on `textDocument/didSave`
                events
              - onType:             Run task on typing, i.e. on `textDocument/didChange` events
              - onDocumentHasTitle: *DEPRECATED* Run task when a document has a title and on saved,
                which is useful to filter out template files
              - script:             Checks by running a typst script

      -f, --format <FORMAT>
              Specify the format of the output file, inferred from the extension by default

              Possible values:
              - pdf:    Export to PDF
              - png:    Export to PNG
              - svg:    Export to SVG
              - html:   Export to HTML
              - bundle: Export to Bundle

          --pages <PAGES>
              Specify which pages to export. When unspecified, all pages are exported.
              
              Pages to export are separated by commas, and can be either simple page numbers (e.g. '2,5'
              to export only pages 2 and 5) or page ranges (e.g. '2,3-6,8-' to export page 2, pages 3 to
              6 (inclusive), page 8 and any pages after it).
              
              Page numbers are one-indexed and correspond to physical page numbers in the document
              (therefore not being affected by the document's page counter).

          --pdf-standard <STANDARD>
              Specify the PDF standards that Typst will enforce conformance with.
              
              If multiple standards are specified, they are separated by commas.

              Possible values:
              - 1.4:  PDF 1.4
              - 1.5:  PDF 1.5
              - 1.6:  PDF 1.6
              - 1.7:  PDF 1.7
              - 2.0:  PDF 2.0
              - a-1b: PDF/A-1b
              - a-1a: PDF/A-1a
              - a-2b: PDF/A-2b
              - a-2u: PDF/A-2u
              - a-2a: PDF/A-2a
              - a-3b: PDF/A-3b
              - a-3u: PDF/A-3u
              - a-3a: PDF/A-3a
              - a-4:  PDF/A-4
              - a-4f: PDF/A-4f
              - a-4e: PDF/A-4e
              - ua-1: PDF/UA-1

          --no-pdf-tags
              By default, even when not producing a `PDF/UA-1` document, a tagged PDF document is
              written to provide a baseline of accessibility. In some circumstances (for example when
              trying to reduce the size of a document) it can be desirable to disable tagged PDF

          --ppi <PPI>
              Specify the PPI (pixels per inch) to use for PNG export
              
              [default: 144]

          --save-lock
              Save the compilation arguments to the lock file. If `--lockfile` is not set, the lock file
              will be saved in the cwd

          --lockfile <LOCKFILE>
              Specify the path to the lock file. If the path is set, the lockfile will be saved
              (--save-lock)

          --watch
              Watch the input files and export again on every change, like `typst watch`. Prints
              `compiled successfully` after each export

          --on-export <COMMAND>
              Run a command after every successful export in watch mode, with the path of the written
              file as its last argument and in the environment variable `TINYMIST_OUTPUT`. The command
              runs through the system shell; a failure is reported but does not stop watching

      -h, --help
              Print help (see a summary with '-h')

    ----- stderr -----
    ");
}

#[test]
fn test_help_compile() {
    apply_common_filters!();
    insta_cmd::assert_cmd_snapshot!(cli().arg("compile").arg("--help"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Run compile command like `typst-cli compile`

    Usage: tinymist compile [OPTIONS] <INPUT> [OUTPUT]

    Arguments:
      <INPUT>
              Specify the path to input Typst file

      [OUTPUT]
              Provide the path to output file (PDF, PNG, SVG, or HTML). Use `-` to write output to
              stdout.
              
              For output formats emitting one file per page (PNG & SVG), a page number template must be
              present if the source document renders to multiple pages. Use `{p}` for page numbers,
              `{0p}` for zero padded page numbers and `{t}` for page count. For example,
              `page-{0p}-of-{t}.png` creates `page-01-of-10.png`, `page-02-of-10.png`, and so on.

    Options:
          --name <NAME>
              Give a task name to the document

          --root <DIR>
              Configure the project root (for absolute paths). If the path is relative, it will be
              resolved relative to the current working directory (PWD)
              
              [env: TYPST_ROOT=REDACTED]

          --input <key=value>
              Add a string key-value pair visible through `sys.inputs`

          --font-path <DIR>
              Add additional directories that are recursively searched for fonts.
              
              If multiple paths are specified, they are separated by the system's path separator (`:` on
              Unix-like systems and `;` on Windows).
              
              [env: TYPST_FONT_PATHS=REDACTED]

          --ignore-system-fonts
              Ensure system fonts won't be searched, unless explicitly included via `--font-path`

          --package-path <DIR>
              Specify a custom path to local packages, defaults to system-dependent location
              
              [env: TYPST_PACKAGE_PATH=REDACTED]

          --package-cache-path <DIR>
              Specify a custom path to package cache, defaults to system-dependent location
              
              [env: TYPST_PACKAGE_CACHE_PATH=REDACTED]

          --when <WHEN>
              Configure when to run the task

              Possible values:
              - never:              Never watch to run task
              - onSave:             Run task on saving the document, i.e. on `textDocument/didSave`
                events
              - onType:             Run task on typing, i.e. on `textDocument/didChange` events
              - onDocumentHasTitle: *DEPRECATED* Run task when a document has a title and on saved,
                which is useful to filter out template files
              - script:             Checks by running a typst script

      -f, --format <FORMAT>
              Specify the format of the output file, inferred from the extension by default

              Possible values:
              - pdf:    Export to PDF
              - png:    Export to PNG
              - svg:    Export to SVG
              - html:   Export to HTML
              - bundle: Export to Bundle

          --pages <PAGES>
              Specify which pages to export. When unspecified, all pages are exported.
              
              Pages to export are separated by commas, and can be either simple page numbers (e.g. '2,5'
              to export only pages 2 and 5) or page ranges (e.g. '2,3-6,8-' to export page 2, pages 3 to
              6 (inclusive), page 8 and any pages after it).
              
              Page numbers are one-indexed and correspond to physical page numbers in the document
              (therefore not being affected by the document's page counter).

          --pdf-standard <STANDARD>
              Specify the PDF standards that Typst will enforce conformance with.
              
              If multiple standards are specified, they are separated by commas.

              Possible values:
              - 1.4:  PDF 1.4
              - 1.5:  PDF 1.5
              - 1.6:  PDF 1.6
              - 1.7:  PDF 1.7
              - 2.0:  PDF 2.0
              - a-1b: PDF/A-1b
              - a-1a: PDF/A-1a
              - a-2b: PDF/A-2b
              - a-2u: PDF/A-2u
              - a-2a: PDF/A-2a
              - a-3b: PDF/A-3b
              - a-3u: PDF/A-3u
              - a-3a: PDF/A-3a
              - a-4:  PDF/A-4
              - a-4f: PDF/A-4f
              - a-4e: PDF/A-4e
              - ua-1: PDF/UA-1

          --no-pdf-tags
              By default, even when not producing a `PDF/UA-1` document, a tagged PDF document is
              written to provide a baseline of accessibility. In some circumstances (for example when
              trying to reduce the size of a document) it can be desirable to disable tagged PDF

          --ppi <PPI>
              Specify the PPI (pixels per inch) to use for PNG export
              
              [default: 144]

          --save-lock
              Save the compilation arguments to the lock file. If `--lockfile` is not set, the lock file
              will be saved in the cwd

          --lockfile <LOCKFILE>
              Specify the path to the lock file. If the path is set, the lockfile will be saved
              (--save-lock)

          --watch
              Watch the input files and export again on every change, like `typst watch`. Prints
              `compiled successfully` after each export

          --on-export <COMMAND>
              Run a command after every successful export in watch mode, with the path of the written
              file as its last argument and in the environment variable `TINYMIST_OUTPUT`. The command
              runs through the system shell; a failure is reported but does not stop watching

      -h, --help
              Print help (see a summary with '-h')

    ----- stderr -----
    ");
}

#[test]
fn test_help_preview() {
    apply_common_filters!();
    insta_cmd::assert_cmd_snapshot!(cli().arg("preview").arg("--help"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    Run preview server

    Usage: tinymist preview [OPTIONS] [INPUT]

    Arguments:
      [INPUT]
              Specify the path to input Typst file. If the path is relative, it will be resolved
              relative to the current working directory (PWD)

    Options:
          --format <FORMAT>
              Configure the preview output format.
              
              `tinymist preview` does not write an output file, so this selects the Typst compilation
              target used by the live preview.

              Possible values:
              - paged:  The current export target is for PDF, PNG, and SVG export
              - html:   The current export target is for HTML export
              - bundle: The current export target is for bundle export
              
              [default: paged]

          --preview-mode <MODE>
              Configure the preview mode

              Possible values:
              - document: Would like to preview a regular document
              - slide:    Would like to preview slides
              
              [default: document]

          --page-title <TITLE>
              Set the preview page title.
              
              If not specified, the title falls back to the input filename when available, or otherwise
              to `"Typst Preview"`.

          --partial-rendering <ENABLE_PARTIAL_RENDERING>
              Only render visible part of the document.
              
              This can improve performance but still being experimental.
              
              [possible values: true, false]

          --invert-colors <INVERT_COLORS>
              Configure the way to invert colors of the preview.
              
              This is useful for dark themes without cost.
              
              Please note you could see the original colors when you hover elements in the preview.
              
              It is also possible to specify strategy to each element kind by an object map in JSON
              format.
              
              Possible element kinds: - `image`: Images in the preview. - `rest`: Rest elements in the
              preview.
              
              By default, the preview will never invert colors.
              
              ## Example
              
              By string:
              
              ```shell --invert-colors=auto ```
              
              By element:
              
              ```shell --invert-colors='{"rest": "always", "image": "never"}' ```

          --root <DIR>
              Configure the project root (for absolute paths)

          --font-path <DIR>
              Add additional directories that are recursively searched for fonts.
              
              If multiple paths are specified, they are separated by the system's path separator (`:` on
              Unix-like systems and `;` on Windows).
              
              [env: TYPST_FONT_PATHS=REDACTED]

          --ignore-system-fonts
              Ensure system fonts won't be searched, unless explicitly included via `--font-path`

          --package-path <DIR>
              Specify a custom path to local packages, defaults to system-dependent location
              
              [env: TYPST_PACKAGE_PATH=REDACTED]

          --package-cache-path <DIR>
              Specify a custom path to package cache, defaults to system-dependent location
              
              [env: TYPST_PACKAGE_CACHE_PATH=REDACTED]

          --pdf-standard <STANDARD>
              Specify the PDF standards that Typst will enforce conformance with.
              
              If multiple standards are specified, they are separated by commas.

              Possible values:
              - 1.4:  PDF 1.4
              - 1.5:  PDF 1.5
              - 1.6:  PDF 1.6
              - 1.7:  PDF 1.7
              - 2.0:  PDF 2.0
              - a-1b: PDF/A-1b
              - a-1a: PDF/A-1a
              - a-2b: PDF/A-2b
              - a-2u: PDF/A-2u
              - a-2a: PDF/A-2a
              - a-3b: PDF/A-3b
              - a-3u: PDF/A-3u
              - a-3a: PDF/A-3a
              - a-4:  PDF/A-4
              - a-4f: PDF/A-4f
              - a-4e: PDF/A-4e
              - ua-1: PDF/UA-1

          --no-pdf-tags
              By default, even when not producing a `PDF/UA-1` document, a tagged PDF document is
              written to provide a baseline of accessibility. In some circumstances (for example when
              trying to reduce the size of a document) it can be desirable to disable tagged PDF

          --ppi <PPI>
              Specify the PPI (pixels per inch) to use for PNG export
              
              [default: 144]

          --features <FEATURES>
              Enable in-development features that may be changed or removed at any time

              Possible values:
              - html:        The HTML feature
              - a11y-extras: The A11yExtras feature
              - bundle:      The bundle export feature
              
              [env: TYPST_FEATURES=REDACTED]

          --input <key=value>
              Add a string key-value pair visible through `sys.inputs`.
              
              ### Examples
              
              Tell the script that `sys.inputs.foo` is `"bar"` (type: `str`).
              
              ```bash tinymist compile --input foo=bar ```

          --cert <CERT_PATH>
              Specify the path to CA certificate file for network access, especially for downloading
              typst packages
              
              [env: TYPST_CERT=REDACTED]

          --host <HOST>
              (Deprecated) Configure (File) Host address for the preview server.
              
              Note: if it equals to `data_plane_host`, same address will be used.
              
              [default: ""]

          --open
              Open the preview in the browser after compilation. If `--no-open` is set, this flag will
              be ignored

          --no-open
              Don't open the preview in the browser after compilation. If `--open` is set as well, this
              flag will win

          --verbose
              Emit INFO level logging. The default is WARN

      -h, --help
              Print help (see a summary with '-h')

    ----- stderr -----
    "#);
}
#[test]
fn test_compile() {
    const INPUT_REL: &str = "tests/workspaces/individuals/tiny.typ";

    std::env::set_var("RUST_BACKTRACE", "full");
    let cwd = GIT_ROOT.clone();
    let root = cwd.join("target/e2e/tinymist-cli");

    std::env::set_current_dir(&cwd).expect("should change current directory");
    tinymist_std::fs::paths::temp_dir_in(root, |tmp| {
        let abs_out = tmp.clean();
        let rel_out = abs_out.strip_prefix(&cwd).expect("path should be stripped");

        assert!(cwd.is_absolute(), "cwd should be absolute {cwd:?}");
        assert!(abs_out.is_absolute(), "abs_out should be absolute {abs_out:?}");
        assert!(rel_out.is_relative(), "rel_out should be relative {rel_out:?}");

        // absolute INPUT, absolute OUTPUT
        insta_cmd::assert_cmd_snapshot!(cli().arg("compile").arg(cwd.join(INPUT_REL)).arg(abs_out.join("test1.pdf")), @"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        ");
        // absolute INPUT, relative OUTPUT
        insta_cmd::assert_cmd_snapshot!(cli().arg("compile").arg(cwd.join(INPUT_REL)).arg(rel_out.join("test2.pdf")), @"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        ");
        // relative INPUT, absolute OUTPUT
        insta_cmd::assert_cmd_snapshot!(cli().arg("compile").arg(INPUT_REL).arg(abs_out.join("test3.pdf")), @"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        ");
        // relative INPUT, relative OUTPUT
        insta_cmd::assert_cmd_snapshot!(cli().arg("compile").arg(INPUT_REL).arg(rel_out.join("test4.pdf")), @"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        ");

        for i in 1..=4 {
            let output = rel_out.join(format!("test{i}.pdf"));
            assert!(output.exists(), "output file should exist: {output:?}");
        }

        Ok(())
    })
    .expect("test should succeed");
}

#[test]
fn test_compile_alias() {
    const INPUT_REL: &str = "tests/workspaces/individuals/tiny.typ";

    std::env::set_var("RUST_BACKTRACE", "full");
    let cwd = GIT_ROOT.clone();
    let root = cwd.join("target/e2e/tinymist-cli");

    std::env::set_current_dir(&cwd).expect("should change current directory");
    tinymist_std::fs::paths::temp_dir_in(root, |tmp| {
        let abs_out = tmp.clean();
        let rel_out = abs_out.strip_prefix(&cwd).expect("path should be stripped");

        assert!(cwd.is_absolute(), "cwd should be absolute {cwd:?}");
        assert!(abs_out.is_absolute(), "abs_out should be absolute {abs_out:?}");
        assert!(rel_out.is_relative(), "rel_out should be relative {rel_out:?}");

        // Test the 'c' alias with relative INPUT and OUTPUT
        insta_cmd::assert_cmd_snapshot!(cli().arg("c").arg(INPUT_REL).arg(rel_out.join("test_alias.pdf")), @"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        ");

        let output = rel_out.join("test_alias.pdf");
        assert!(output.exists(), "output file should exist: {output:?}");

        Ok(())
    })
    .expect("test should succeed");
}

/// A running `compile --watch`, killed when the test ends, on failure as well.
struct WatchProcess {
    child: std::process::Child,
    stderr_lines: std::sync::mpsc::Receiver<String>,
}

impl WatchProcess {
    fn start(command: &mut std::process::Command) -> Self {
        use std::io::{BufRead, BufReader};
        use std::process::Stdio;

        let mut child = command
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("should start the watch process");
        let stderr = child.stderr.take().expect("stderr should be piped");

        // Status lines arrive on stderr; a thread forwards them so that the
        // test can wait with a timeout.
        let (tx, stderr_lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });

        Self {
            child,
            stderr_lines,
        }
    }

    /// Returns the next status line, or `None` if none arrives in time.
    fn next_status(&self, timeout: std::time::Duration) -> Option<String> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match self.stderr_lines.recv_timeout(left) {
                Ok(line) if line.starts_with("compiled ") || line.starts_with("export failed") => {
                    return Some(line);
                }
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }
}

impl Drop for WatchProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Writes an executable shell script.
fn write_hook(path: &std::path::Path, script: &str) {
    std::fs::write(path, script).expect("should write hook");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .expect("should make hook executable");
    }
}

#[test]
fn test_compile_watch() {
    use std::time::Duration;

    let cwd = GIT_ROOT.clone();
    let root = cwd.join("target/e2e/tinymist-cli");

    std::env::set_current_dir(&cwd).expect("should change current directory");
    tinymist_std::fs::paths::temp_dir_in(root, |tmp| {
        // A private copy of the book workspace, since the test edits a chapter.
        let tmp = tmp.clean();
        let fixture = cwd.join("tests/workspaces/book");
        let book = tmp.join("workspace");
        std::fs::create_dir_all(book.join("chapters")).expect("should create workspace");
        for rel in ["main.typ", "chapters/chapter1.typ", "chapters/chapter2.typ"] {
            std::fs::copy(fixture.join(rel), book.join(rel)).expect("should copy fixture");
        }
        let out = tmp.join("book.pdf");

        // On Unix, a hook script records every export.
        let exports_log = tmp.join("exports.log");
        let hook = tmp.join("hook.sh");
        let mut command = cli();
        command
            .arg("compile")
            .arg("--watch")
            .arg("--root")
            .arg(&book)
            .arg(book.join("main.typ"))
            .arg(&out);
        if cfg!(unix) {
            let script = format!(
                "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"{}\"\n",
                exports_log.display()
            );
            write_hook(&hook, &script);
            command.arg("--on-export").arg(&hook);
        }

        let watch = WatchProcess::start(&mut command);

        let first_status = watch.next_status(Duration::from_secs(120));
        assert_eq!(first_status.as_deref(), Some("compiled successfully"));
        let first = std::fs::read(&out).expect("pdf should exist after the first compilation");

        // Editing a dependency triggers another compilation and export. The
        // file watches are installed shortly after the first compilation, so
        // an edit that comes too early is repeated.
        let chapter = book.join("chapters/chapter2.typ");
        let mut second_status = None;
        for _ in 0..6 {
            let mut text = std::fs::read_to_string(&chapter).expect("should read chapter");
            text.push_str("\nAppended while watching.\n");
            std::fs::write(&chapter, text).expect("should edit chapter");
            second_status = watch.next_status(Duration::from_secs(10));
            if second_status.is_some() {
                break;
            }
        }
        assert_eq!(second_status.as_deref(), Some("compiled successfully"));
        let second = std::fs::read(&out).expect("pdf should exist after the second compilation");
        assert_ne!(
            first, second,
            "the pdf should be exported again with the edited chapter"
        );

        // A slow compilation can make the loop above edit twice, so this
        // counts at least two exports.
        if cfg!(unix) {
            let log = std::fs::read_to_string(&exports_log).expect("hook should have run");
            let exports: Vec<&str> = log.lines().collect();
            assert!(exports.len() >= 2, "hook should run per export: {log}");
            assert!(
                exports.iter().all(|path| *path == out.to_str().unwrap()),
                "hook should get the written file: {log}"
            );
        }

        Ok(())
    })
    .expect("test should succeed");
}

/// Changes that arrive during an export wait for it, so hooks never overlap.
#[cfg(unix)]
#[test]
fn test_compile_watch_exports_one_at_a_time() {
    use std::time::Duration;

    let cwd = GIT_ROOT.clone();
    let root = cwd.join("target/e2e/tinymist-cli");

    std::env::set_current_dir(&cwd).expect("should change current directory");
    tinymist_std::fs::paths::temp_dir_in(root, |tmp| {
        let tmp = tmp.clean();
        let main = tmp.join("main.typ");
        std::fs::write(&main, "= Heading\n").expect("should write document");

        let exports_log = tmp.join("exports.log");
        let hook = tmp.join("hook.sh");
        let log = exports_log.display();
        let script =
            format!("#!/bin/sh\necho start >> \"{log}\"\nsleep 1\necho end >> \"{log}\"\n");
        write_hook(&hook, &script);

        let watch = WatchProcess::start(
            cli()
                .arg("compile")
                .arg("--watch")
                .arg("--root")
                .arg(&tmp)
                .arg(&main)
                .arg(tmp.join("out.pdf"))
                .arg("--on-export")
                .arg(&hook),
        );
        let first_status = watch.next_status(Duration::from_secs(120));
        assert_eq!(first_status.as_deref(), Some("compiled successfully"));

        // Three changes inside the duration of one hook.
        for _ in 0..3 {
            let mut text = std::fs::read_to_string(&main).expect("should read document");
            text.push_str("More text.\n");
            std::fs::write(&main, text).expect("should edit document");
            std::thread::sleep(Duration::from_millis(300));
        }
        while watch.next_status(Duration::from_secs(3)).is_some() {}

        let log = std::fs::read_to_string(&exports_log).expect("hook should have run");
        let events: Vec<&str> = log.lines().collect();
        assert!(events.len() > 2, "changes should export again: {log}");
        for event in events.chunks(2) {
            assert_eq!(event, ["start", "end"], "hooks should not overlap: {log}");
        }

        Ok(())
    })
    .expect("test should succeed");
}

/// `--save-lock` records the same project as in a run without `--watch`.
#[test]
fn test_compile_watch_saves_lock() {
    use std::time::Duration;

    let cwd = GIT_ROOT.clone();
    let root = cwd.join("target/e2e/tinymist-cli");

    std::env::set_current_dir(&cwd).expect("should change current directory");
    tinymist_std::fs::paths::temp_dir_in(root, |tmp| {
        let tmp = tmp.clean();
        std::fs::write(tmp.join("main.typ"), "= Heading\n").expect("should write document");
        let lock = tmp.join("tinymist.lock");

        let status = cli()
            .current_dir(&tmp)
            .args(["compile", "--save-lock", "main.typ", "out.pdf"])
            .status()
            .expect("should run compile");
        assert!(status.success());
        let one_shot = std::fs::read_to_string(&lock).expect("should write lock file");
        std::fs::remove_file(&lock).expect("should remove lock file");

        let mut command = cli();
        command.current_dir(&tmp).args([
            "compile",
            "--watch",
            "--save-lock",
            "main.typ",
            "out.pdf",
        ]);
        let watch = WatchProcess::start(&mut command);
        let status = watch.next_status(Duration::from_secs(120));
        assert_eq!(status.as_deref(), Some("compiled successfully"));

        let watched = std::fs::read_to_string(&lock).expect("should write lock file");
        assert_eq!(watched, one_shot);

        Ok(())
    })
    .expect("test should succeed");
}
