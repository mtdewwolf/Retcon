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
$authOutput = & $claude.Source auth status 2>$null
try {
    $auth = ($authOutput -join "`n") | ConvertFrom-Json
} catch {
    Pop-Location
    Write-Host "FAIL (could not parse Claude authentication status)"
    exit 1
}
if (-not $auth.loggedIn) {
    Pop-Location
    Write-Host "FAIL (Claude Code is not authenticated)"
    exit 1
}

Write-Host "Live turn: stream text and one read-only tool event..." -ForegroundColor Cyan
$prompt = "Use the Bash tool exactly once to run git rev-parse --show-toplevel, then reply with only RETCON_AGENT_E2E_OK. Do not modify files."
$turnOutput = & $claude.Source -p $prompt --output-format stream-json --verbose --allowedTools Bash 2>&1
if ($LASTEXITCODE -ne 0) {
    Pop-Location
    Write-Host "FAIL (live Claude turn exited $LASTEXITCODE)"
    exit 1
}
$turnText = $turnOutput -join "`n"
$sessionMatch = [regex]::Match($turnText, '"session_id":"([A-Za-z0-9-]+)"')
if (-not $sessionMatch.Success -or $turnText -notmatch '"type":"tool_use"' -or $turnText -notmatch 'RETCON_AGENT_E2E_OK') {
    Pop-Location
    Write-Host "FAIL (live turn did not include session, tool, and completion evidence)"
    exit 1
}
$sessionId = $sessionMatch.Groups[1].Value

Write-Host "Native session resume..." -ForegroundColor Cyan
$resumeOutput = & $claude.Source -p "Reply with only RETCON_AGENT_RESUME_OK." --output-format stream-json --verbose --resume $sessionId 2>&1
if ($LASTEXITCODE -ne 0 -or ($resumeOutput -join "`n") -notmatch 'RETCON_AGENT_RESUME_OK') {
    Pop-Location
    Write-Host "FAIL (native session resume)"
    exit 1
}

Write-Host "Adapter cancellation..." -ForegroundColor Cyan
cargo test -p retcon-agents authenticated_claude_turn_can_be_cancelled -- --ignored --nocapture
$cancelExit = $LASTEXITCODE
Pop-Location
if ($cancelExit -ne 0) {
    Write-Host "FAIL (adapter cancellation)"
    exit 1
}

Write-Host "PASS (authenticated turn + tool event + cancellation + native resume; raw output intentionally not persisted)"
exit 0
