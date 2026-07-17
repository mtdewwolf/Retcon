# Matrix item 1: Windows 10/11 shell - custom controls, drag, resize, maximize/restore,
# snap layouts, dark menus, DPI scaling. Requires human operator on real Windows hardware.
param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
)

$ErrorActionPreference = "Stop"
Write-Host "Phase 2 matrix #1 - Windows shell (interactive)" -ForegroundColor Cyan
Write-Host "Code under test: apps/desktop/lib/src/desktop_shell.dart, window_controller.dart"
Write-Host ""

$checks = @(
    "Launch Retcon desktop (flutter run -d windows from apps/desktop)",
    "Verify custom title-bar controls: minimize, maximize/restore, close",
    "Drag window by title bar; resize from edges",
    "Maximize, restore, and exercise Snap Layouts (Win+Arrow)",
    "Open each top-level menu; confirm dark theme styling",
    "Set display scaling to 125% and 150%; confirm readable chrome at minimum size (760x480)",
    "Repeat on Windows 10 if available, or note Win11-only evidence"
)

Write-Host "Checklist (record screen + note DPI/OS build):" -ForegroundColor Yellow
$index = 1
foreach ($item in $checks) {
    Write-Host "  [$index] $item"
    $index++
}

Write-Host ""
Write-Host "Automated proxy (widget tests - not a substitute for native shell):" -ForegroundColor Cyan
Push-Location (Join-Path $RepoRoot "apps/desktop")
flutter test test/desktop_shell_test.dart
$flutterExit = $LASTEXITCODE
Pop-Location

if ($flutterExit -ne 0) {
    Write-Host "FAIL (widget tests failed)"
    exit 1
}

Write-Host ""
Write-Host "Widget tests passed. Complete the interactive checklist above, then re-run with -ConfirmInteractive."
Write-Host "PASS (automated proxy only - interactive checklist pending human sign-off)"
exit 0
