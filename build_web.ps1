# Builds the browser version into web/. Serve it with: python -m http.server -d web 8000
cargo build -p game --release --target wasm32-unknown-unknown
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Copy-Item target/wasm32-unknown-unknown/release/game.wasm web/game.wasm -Force
Write-Host "Built web/game.wasm. Serve with: python -m http.server -d web 8000"
