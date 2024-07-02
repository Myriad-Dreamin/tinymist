$InstallPath = ".\editors\vscode\out"
if (-Not (Test-Path $InstallPath)) {
    New-Item -ItemType Directory $InstallPath
}
  
cargo build --release --bin tinymist
Copy-Item -Path ".\target\release\tinymist.exe" -Destination "$InstallPath\tinymist.exe" -Force
cargo insta test -p tests --accept
