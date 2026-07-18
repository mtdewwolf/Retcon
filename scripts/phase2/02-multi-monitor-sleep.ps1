# Matrix item 2: Two-monitor mixed-DPI setup and sleep/resume layout persistence.
# Captures workspace layout JSON before/after; human verifies visual placement on monitors.
param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path,
    [string]$OutputDir = (Join-Path $PSScriptRoot "artifacts")
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

Write-Host "Phase 2 matrix #2 - multi-monitor / sleep-resume" -ForegroundColor Cyan
Write-Host "Code under test: apps/desktop/lib/src/workspace.dart"
Write-Host ""

Push-Location (Join-Path $RepoRoot "apps/desktop")
flutter test test/workspace_test.dart
$testExit = $LASTEXITCODE
Pop-Location

if ($testExit -ne 0) {
    Write-Host "FAIL (workspace unit tests)"
    exit 1
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$layoutFile = Join-Path $OutputDir "workspace-layout-$stamp.json"

# Sample layout matching WorkspaceLayout.initial() wire format for operator diffing.
$sampleLayout = @{
    version = 1
    root    = @{
        type         = "split"
        axis         = "horizontal"
        fraction     = 0.24
        first        = @{ type = "tabs"; activeIndex = 0; panels = @(@{ id = "explorer"; title = "Project explorer"; icon = "folder" }) }
        second       = @{ type = "tabs"; activeIndex = 0; panels = @(@{ id = "workspace"; title = "Workspace"; icon = "dashboard" }) }
    }
    floatingPanels = @()
    closedPanels   = @()
} | ConvertTo-Json -Depth 8

Set-Content -Path $layoutFile -Value $sampleLayout -Encoding utf8
Write-Host "Wrote sample layout snapshot: $layoutFile"

Write-Host ""
Write-Host "Interactive steps:" -ForegroundColor Yellow
Write-Host "  1. Arrange panels across two monitors at different DPI (e.g. 100% + 150%)."
Write-Host "  2. Copy .retcon-workspace.json (or export via app) to artifacts/ as before-sleep.json."
Write-Host "  3. Sleep/resume (or disconnect/reconnect external monitor)."
Write-Host "  4. Copy post-resume layout to artifacts/after-resume.json and diff."
Write-Host ""

Write-Host "PASS (serialization tests; interactive monitor/sleep evidence pending)"
exit 0
