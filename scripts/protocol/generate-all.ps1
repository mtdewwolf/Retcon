$ErrorActionPreference = "Stop"
$Root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
Set-Location $Root
node scripts/protocol/validate.mjs
node scripts/protocol/generate-typescript.mjs
node scripts/protocol/generate-dart.mjs
Write-Host "Protocol codegen complete."
