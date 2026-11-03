use std::io::Read;
use std::path::Path;

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

          --synctex
              Write a `.synctex.gz` file next to the PDF, mapping PDF positions to source lines for
              viewers such as Skim or Okular. Only for PDF output of all pages

          --ppi <PPI>
              Specify the PPI (pixels per inch) to use for PNG export
              
              [default: 144]

          --save-lock
              Save the compilation arguments to the lock file. If `--lockfile` is not set, the lock file
              will be saved in the cwd

          --lockfile <LOCKFILE>
              Specify the path to the lock file. If the path is set, the lockfile will be saved
              (--save-lock)

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

          --synctex
              Write a `.synctex.gz` file next to the PDF, mapping PDF positions to source lines for
              viewers such as Skim or Okular. Only for PDF output of all pages

          --ppi <PPI>
              Specify the PPI (pixels per inch) to use for PNG export
              
              [default: 144]

          --save-lock
              Save the compilation arguments to the lock file. If `--lockfile` is not set, the lock file
              will be saved in the cwd

          --lockfile <LOCKFILE>
              Specify the path to the lock file. If the path is set, the lockfile will be saved
              (--save-lock)

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

/// Reads a SyncTeX file, which is gzip-compressed text.
fn read_synctex(path: &Path) -> String {
    let file = std::fs::File::open(path).expect("synctex file should exist");
    let mut text = String::new();
    flate2::read::GzDecoder::new(file)
        .read_to_string(&mut text)
        .expect("synctex file should be gzip-compressed text");
    text
}

/// Scaled points, the unit of SyncTeX coordinates.
fn sp(pt: f64) -> i64 {
    (pt * 65536.0 * 72.27 / 72.0).round() as i64
}

/// The source line and position of an hbox record `(tag,line:x,y:w,h,d`.
fn hbox(record: &str) -> (usize, i64, i64) {
    let (_, rest) = record.split_once(',').expect("record should have a tag");
    let mut fields = rest.split([':', ',']);
    let mut field = || fields.next().expect("record should have all fields");
    (
        field().parse().expect("line"),
        field().parse().expect("x"),
        field().parse().expect("y"),
    )
}

#[test]
fn test_compile_synctex_records() {
    const SOURCE: &str = "\
#set page(width: 300pt, height: 300pt, margin: 0pt)
#outline()

= Alpha

first source line
second source line

#place(top + left, dx: 100pt, dy: 200pt, scale(200%, origin: top + left)[Scaled])
";
    const HEADING: usize = 4;
    const FIRST: usize = 6;
    const SECOND: usize = 7;
    const SCALED: usize = 9;

    let root = GIT_ROOT.join("target/e2e/tinymist-cli");
    tinymist_std::fs::paths::temp_dir_in(root, |tmp| {
        let tmp = tmp.clean();
        let input = tmp.join("main.typ");
        let out = tmp.join("main.pdf");
        std::fs::write(&input, SOURCE).expect("should write the source");

        let status = cli()
            .arg("compile")
            .arg("--synctex")
            .arg(&input)
            .arg(&out)
            .status();
        assert!(status.expect("should run").success());

        let text = read_synctex(&out.with_extension("synctex.gz"));
        let hboxes = text
            .lines()
            .filter(|l| l.starts_with('('))
            .map(hbox)
            .collect::<Vec<_>>();
        let on_line = |line| hboxes.iter().filter(|h| h.0 == line).collect::<Vec<_>>();

        // The outline renders "Alpha" as well, with the heading's span.
        assert_eq!(
            on_line(HEADING).len(),
            1,
            "only the heading itself: {hboxes:?}"
        );

        // One typeset line from two source lines gives one record per source line.
        let (first, second) = (on_line(FIRST), on_line(SECOND));
        assert_eq!((first.len(), second.len()), (1, 1), "{hboxes:?}");
        assert_eq!(first[0].2, second[0].2, "both on the same typeset line");
        assert!(first[0].1 < second[0].1, "second source line further right");

        // The scaled text sits at the place offset, not at the origin of its group.
        let scaled = on_line(SCALED);
        assert_eq!(scaled.len(), 1, "{hboxes:?}");
        assert_eq!(scaled[0].1, sp(100.0));
        assert!(scaled[0].2 > sp(200.0), "baseline below the place offset");

        Ok(())
    })
    .expect("test should succeed");
}

#[test]
fn test_compile_synctex_only_for_correct_output() {
    let root = GIT_ROOT.join("target/e2e/tinymist-cli");
    tinymist_std::fs::paths::temp_dir_in(root, |tmp| {
        let tmp = tmp.clean();
        let input = tmp.join("main.typ");
        std::fs::write(&input, "Hello\n").expect("should write the source");

        let synctex_of = |name: &str| tmp.join(name).with_extension("synctex.gz");
        let compile = |args: &[&str], name: &str| {
            let status = cli()
                .arg("compile")
                .args(args)
                .arg(&input)
                .arg(tmp.join(name))
                .status();
            status.expect("should run").success()
        };

        assert!(compile(&[], "plain.pdf"));
        assert!(
            !synctex_of("plain.pdf").exists(),
            "no file without the flag"
        );

        // Neither output can have a correct SyncTeX file, so they must not
        // silently go without one or get a wrong one.
        assert!(!compile(&["--synctex"], "image.png"));
        assert!(!synctex_of("image.png").exists());
        assert!(!compile(&["--synctex", "--pages", "1"], "subset.pdf"));
        assert!(!synctex_of("subset.pdf").exists());

        std::fs::write(&input, "#panic()\n").expect("should write the source");
        assert!(!compile(&["--synctex"], "broken.pdf"));
        assert!(
            !synctex_of("broken.pdf").exists(),
            "no file for a failed compilation"
        );

        Ok(())
    })
    .expect("test should succeed");
}

#[test]
fn test_generate_script_keeps_synctex() {
    let root = GIT_ROOT.join("target/e2e/tinymist-cli");
    tinymist_std::fs::paths::temp_dir_in(root, |tmp| {
        let tmp = tmp.clean();
        std::fs::write(tmp.join("main.typ"), "Hello\n").expect("should write the source");

        let run = |args: &[&str]| cli().current_dir(&tmp).args(args).status();
        let compile = run(&["compile", "--save-lock", "--synctex", "main.typ"]);
        assert!(compile.expect("should run").success());
        assert!(run(&["generate-script", "bash"])
            .expect("should run")
            .success());

        let script = std::fs::read_to_string(tmp.join("build.sh")).expect("script should exist");
        assert!(
            script.contains("--synctex"),
            "script should keep the flag: {script}"
        );

        Ok(())
    })
    .expect("test should succeed");
}

#[test]
fn test_compile_synctex() {
    const INPUT_REL: &str = "tests/workspaces/book/main.typ";

    let cwd = GIT_ROOT.clone();
    let root = cwd.join("target/e2e/tinymist-cli");

    std::env::set_current_dir(&cwd).expect("should change current directory");
    tinymist_std::fs::paths::temp_dir_in(root, |tmp| {
        let out = tmp.clean().join("book.pdf");

        insta_cmd::assert_cmd_snapshot!(cli().arg("compile").arg("--synctex").arg(INPUT_REL).arg(&out), @"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        ");
        assert!(out.exists(), "pdf should exist: {out:?}");

        let text = read_synctex(&out.with_extension("synctex.gz"));

        // The header names every file that contributed text, with absolute
        // paths; main.typ only includes the chapters and has no text of its own.
        let header = text
            .lines()
            .take_while(|line| !line.starts_with("Content:"))
            .collect::<Vec<_>>()
            .join("\n")
            .replace(&*cwd.to_string_lossy(), "<root>")
            .replace('\\', "/");
        insta::assert_snapshot!(header, @"
        SyncTeX Version:1
        Input:1:<root>/tests/workspaces/book/chapters/chapter1.typ
        Input:2:<root>/tests/workspaces/book/chapters/chapter2.typ
        Output:pdf
        Magnification:1000
        Unit:1
        X Offset:0
        Y Offset:0
        ");

        // One page block with a page box and one hbox per text run; the
        // heading of chapter 1 is on line 2 of its file, the lorem text on
        // line 4.
        assert!(text.contains("\n{1\n[1,2:"), "page 1 should start with the page box");
        assert!(text.contains("\n}1\n"), "page 1 should be closed");
        let hboxes = text.lines().filter(|line| line.starts_with('(')).collect::<Vec<_>>();
        assert!(hboxes.len() >= 4, "expected text runs for two headings and two paragraphs, got {hboxes:?}");
        assert!(hboxes.iter().any(|line| line.starts_with("(1,2:")), "heading of chapter 1 on line 2");
        assert!(hboxes.iter().any(|line| line.starts_with("(1,4:")), "text of chapter 1 on line 4");
        assert!(hboxes.iter().any(|line| line.starts_with("(2,2:")), "heading of chapter 2 on line 2");
        assert!(text.contains("\nPostamble:\nCount:"), "postamble should carry a record count");

        Ok(())
    })
    .expect("test should succeed");
}
