# Launch the Retcon test dashboard (Windows).
param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
)

$ErrorActionPreference = "Stop"
$dashboard = Join-Path $RepoRoot "apps/test-dashboard"

Push-Location $dashboard
try {
    $env:RETCON_ROOT = $RepoRoot
    flutter pub get
    flutter run -d windows
} finally {
    Pop-Location
}
