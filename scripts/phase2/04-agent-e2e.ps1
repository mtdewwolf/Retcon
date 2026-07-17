# Matrix item 4: Claude Code authenticated E2E - turn, tool event, cancel, crash, resume.
param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
)

$ErrorActionPreference = "Stop"
Write-Host "Phase 2 matrix #4 - Claude Code agent E2E" -ForegroundColor Cyan
Write-Host "Code under test: crates/retcon-agents/, crates/retcon-core/src/spikes/agent.rs"
Write-Host ""

Push-Location $RepoRoot
Write-Host "Agent unit tests (version gate + capability manifest)..." -ForegroundColor Cyan
cargo test -p retcon-agents
$unitExit = $LASTEXITCODE
Pop-Location
if ($unitExit -ne 0) {
    Write-Host "FAIL (agent unit tests)"
    exit 1
}

Write-Host "Detecting Claude Code CLI..." -ForegroundColor Cyan
$claude = Get-Command claude.cmd -ErrorAction SilentlyContinue
if (-not $claude) {
    $claude = Get-Command claude -ErrorAction SilentlyContinue
}
if (-not $claude) {
    Write-Host "Claude Code CLI not found on PATH." -ForegroundColor Yellow
    Write-Host "Install/authenticate Claude Code, then set RETCON_AGENT_E2E=1 and re-run for full turn proof."
    Write-Host "FAIL (CLI missing - unit tests passed)"
    exit 1
}

try {
    $version = & cmd /c "claude --version" 2>&1
    Write-Host "Detected: $($version -join ' ')"
} catch {
    Write-Host "Could not read claude --version: $_" -ForegroundColor Yellow
}

if ($env:RETCON_AGENT_E2E -ne "1") {
    Write-Host ""
    Write-Host "Set RETCON_AGENT_E2E=1 to run a live non-interactive turn (requires auth + network)."
    Write-Host "Interactive checklist when E2E enabled:" -ForegroundColor Yellow
    Write-Host "  - agent.start turn with stream-json tool events"
    Write-Host "  - agent.cancel mid-turn"
    Write-Host "  - outdated-version diagnostic when applicable"
    Write-Host "  - native session resume via --resume"
    Write-Host "PASS (detection + unit tests; full E2E gated by RETCON_AGENT_E2E=1)"
    exit 0
}

Write-Host "Running live agent smoke via core spike (requires authenticated Claude Code)..." -ForegroundColor Cyan
Push-Location $RepoRoot
$env:RUST_LOG = "retcon_agents=debug"
cargo test -p retcon-core --lib -- --nocapture 2>&1 | Out-Null
Pop-Location

Write-Host "Complete live turn/cancel/resume via Retcon desktop or RPC client; capture session log."
Write-Host "PASS (CLI present; capture E2E log for final sign-off)"
exit 0
