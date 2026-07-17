# Phase 2 automated evidence sweep (developer machine + CI helper).
# Runs: cargo clippy/test, flutter analyze/test, browser-service lint/typecheck/unit tests,
# git matrix script, and agent unit tests. Does not replace interactive matrix items 1-2.
param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
)

$ErrorActionPreference = "Stop"
$failures = @()

function Step($Name, [scriptblock]$Action) {
    Write-Host "`n=== $Name ===" -ForegroundColor Cyan
    try {
        & $Action
        if ($LASTEXITCODE -ne 0 -and $null -ne $LASTEXITCODE) {
            throw "exit code $LASTEXITCODE"
        }
        Write-Host "OK: $Name" -ForegroundColor Green
    } catch {
        Write-Host "FAIL: $Name - $_" -ForegroundColor Red
        $script:failures += $Name
    }
}

Push-Location $RepoRoot
try {
    Step "cargo clippy" { cargo clippy --workspace --all-targets -- -D warnings }
    Step "cargo test" { cargo test --workspace --all-targets }
    Step "flutter analyze" {
        Push-Location (Join-Path $RepoRoot "apps/desktop")
        flutter analyze
        Pop-Location
    }
    Step "flutter test" {
        Push-Location (Join-Path $RepoRoot "apps/desktop")
        flutter test
        Pop-Location
    }
    Step "browser-service lint/typecheck/unit" {
        Push-Location (Join-Path $RepoRoot "apps/browser-service")
        bunx @biomejs/biome check src
        bunx tsc --noEmit
        bun test
        Pop-Location
    }
    Step "git matrix (06)" { & (Join-Path $PSScriptRoot "06-git-large-repo.ps1") }
    Step "agent unit tests" { cargo test -p retcon-agents }
} finally {
    Pop-Location
}

if ($failures.Count -gt 0) {
    Write-Host "`nFAIL automated sweep ($($failures.Count) step(s)):" -ForegroundColor Red
    $failures | ForEach-Object { Write-Host "  - $_" }
    Write-Host "FAIL"
    exit 1
}

Write-Host "`nPASS automated sweep"
exit 0
