
cargo build --release --bin tinymist
Copy-Item -Path ".\target\release\tinymist.exe" -Destination ".\editors\vscode\out\tinymist.exe" -Force
cargo insta test -p tests --accept
