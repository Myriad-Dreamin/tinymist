import os
import subprocess
import sys


def run_tests(test_files=None):
    """
    Run the Neovim test suite with the specified test files.
    If no test files are specified, it runs all tests in the 'spec' directory.
    """
    init_lua = os.path.realpath(
        os.path.join(__file__, "../../scripts/minimal_init.lua")
    )

    if test_files is None:
        # all test files in the 'spec' directory
        test_files = []
        for root, _, files in os.walk(os.path.dirname(__file__)):
            test_files.extend(
                os.path.join(root, f) for f in files if f.endswith("lockfile_spec.lua")
            )
    test_files = " ".join(test_files)

    command = [
        "nvim",
        "--headless",
        "--clean",
        "-u",
        init_lua,
        "-c",
        f'lua require("inanis").run{{ specs = vim.split("{test_files}", " "), minimal_init = "{init_lua}", sequential = vim.env.TEST_SEQUENTIAL ~= nil }}',
    ]

    subprocess.run(command, check=True)


def prepare():
    bookdir = os.environ.get("BOOKDIR", "/home/runner/dev/workspaces/book")
    if not os.path.exists(bookdir):
        print(f"Book directory {bookdir} does not exist.")
        sys.exit(1)

    # compile
    compile_command = [
        "tinymist",
        "compile",
        "--lockfile",
        os.path.join(bookdir, "tinymist.lock"),
        os.path.join(bookdir, "main.typ"),
        os.path.join(bookdir, "book.pdf"),
    ]

    try:
        subprocess.run(compile_command, check=True)
    except subprocess.CalledProcessError as e:
        print(f"Compilation failed: {e}")
        sys.exit(1)

    print("Compilation completed successfully.")


if __name__ == "__main__":
    # Check if any test files are provided as command line arguments
    if len(sys.argv) > 1:
        test_files = " ".join(sys.argv[1:])
    else:
        test_files = None

    prepare()
    run_tests(test_files)
