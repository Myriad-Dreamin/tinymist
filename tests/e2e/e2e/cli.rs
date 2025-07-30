use tinymist_std::path::PathClean;

use crate::artifact::{cli, GIT_ROOT};

#[test]
fn cli_compile() {
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
        insta_cmd::assert_cmd_snapshot!(cli().arg("compile").arg(cwd.join(INPUT_REL)).arg(abs_out.join("test1.pdf")), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        ");
        // absolute INPUT, relative OUTPUT
        insta_cmd::assert_cmd_snapshot!(cli().arg("compile").arg(cwd.join(INPUT_REL)).arg(rel_out.join("test2.pdf")), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        ");
        // relative INPUT, absolute OUTPUT
        insta_cmd::assert_cmd_snapshot!(cli().arg("compile").arg(INPUT_REL).arg(abs_out.join("test3.pdf")), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        ");
        // relative INPUT, relative OUTPUT
        insta_cmd::assert_cmd_snapshot!(cli().arg("compile").arg(INPUT_REL).arg(rel_out.join("test4.pdf")), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        ");

        for i in 1..=4 {
            let output = rel_out.join(format!("test{}.pdf", i));
            assert!(output.exists(), "output file should exist: {output:?}");
        }

        Ok(())
    })
    .expect("test should succeed");
}
