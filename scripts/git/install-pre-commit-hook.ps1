# Install the Retcon secret-scan pre-commit hook in .git/hooks/pre-commit.

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Hook = Join-Path $Root ".git/hooks/pre-commit"
$Scanner = Join-Path $Root "scripts/git/pre-commit-secrets.ps1"

@(
    "#!/usr/bin/env pwsh",
    "Set-StrictMode -Version Latest",
    "`$ErrorActionPreference = 'Stop'",
    "& '$Scanner'"
) | Set-Content -Path $Hook -Encoding utf8

Write-Host "Installed secret scan pre-commit hook at $Hook"
