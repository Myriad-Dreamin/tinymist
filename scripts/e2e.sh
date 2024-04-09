mkdir -p editors/vscode/out/
cargo build --release --bin tinymist
cp target/release/tinymist editors/vscode/out/tinymist
cargo insta test -p tests --accept
