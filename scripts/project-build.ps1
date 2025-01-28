# From file:docs/tinymist/book.typ to file:docs/tinymist/book.typ (pdf)
cargo run --bin tinymist -- compile --save-lock --task 'file:docs/tinymist/book.typ' 'docs/tinymist/book.typ' --root '.' --format=pdf
# From file:docs/tinymist/ebook.typ to ebook-cover (svg)
cargo run --bin tinymist -- compile --save-lock --task 'ebook-cover' 'docs/tinymist/ebook.typ' --root '.' --pages 1-1 --format=svg
# From file:docs/tinymist/ebook.typ to file:docs/tinymist/ebook.typ (pdf)
cargo run --bin tinymist -- compile --save-lock --task 'file:docs/tinymist/ebook.typ' 'docs/tinymist/ebook.typ' --root '.' --format=pdf
