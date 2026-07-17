# Scan staged Git changes for likely secrets before commit.
# Install: scripts/git/install-pre-commit-hook.ps1

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $Root
cargo run --quiet -p retcon-secrets -- scan-staged
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
