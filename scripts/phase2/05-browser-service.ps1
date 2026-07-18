# Matrix item 5: Bun browser service with Playwright-managed Chromium.
param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path,
    [switch]$SkipPlaywrightInstall
)

$ErrorActionPreference = "Stop"
$browserDir = Join-Path $RepoRoot "apps/browser-service"
Write-Host "Phase 2 matrix #5 - managed Chromium browser path" -ForegroundColor Cyan
Write-Host "Code under test: apps/browser-service/"
Write-Host ""

Push-Location $browserDir
bun install --frozen-lockfile 2>$null
if ($LASTEXITCODE -ne 0) { bun install }

if (-not $SkipPlaywrightInstall) {
    Write-Host "Installing Playwright Chromium (may take several minutes)..." -ForegroundColor Cyan
    bunx playwright install chromium
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Playwright install failed; trying system Chrome via RETCON_CHROMIUM_PATH..." -ForegroundColor Yellow
        $chrome = "${env:ProgramFiles}\Google\Chrome\Application\chrome.exe"
        if (Test-Path $chrome) {
            $env:RETCON_CHROMIUM_PATH = $chrome
        }
    }
}

Write-Host "Lint + typecheck + unit tests..." -ForegroundColor Cyan
bunx @biomejs/biome check src
bunx tsc --noEmit
bun test
if ($LASTEXITCODE -ne 0) {
    Pop-Location
    Write-Host "FAIL (unit tests)"
    exit 1
}

Write-Host "E2E: navigate, screenshot, console/network, action, close..." -ForegroundColor Cyan
# Playwright's Windows pipe transport is run under Node. Bun remains the service
# package manager and unit-test runner; direct Playwright launches under Bun can
# hang after the browser process starts without completing the pipe handshake.
bun run test:e2e
$e2eExit = $LASTEXITCODE

Write-Host "Launch proof script..." -ForegroundColor Cyan
bun run browser:verify
$verifyExit = $LASTEXITCODE
Pop-Location

if ($e2eExit -ne 0 -or $verifyExit -ne 0) {
    Write-Host "FAIL (E2E or verify-browser - ensure Playwright Chromium installed or set RETCON_CHROMIUM_PATH)"
    exit 1
}

Write-Host "PASS"
exit 0
