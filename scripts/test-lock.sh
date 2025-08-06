bookdir=tests/workspaces/book
typst compile ${bookdir}/main.typ ${bookdir}/book.pdf
cargo run --bin tinymist -- compile --lockfile ${bookdir}/tinymist.lock ${bookdir}/main.typ ${bookdir}/book.pdf
