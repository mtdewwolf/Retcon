# Matrix item 3: Interactive PowerShell, cmd, Git Bash - ANSI, Unicode, resize, exit, force-stop.
param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path,
    [switch]$SkipIgnoredTest
)

$ErrorActionPreference = "Stop"
Write-Host "Phase 2 matrix #3 - interactive terminals" -ForegroundColor Cyan
Write-Host "Code under test: crates/retcon-terminal/, crates/retcon-core/src/spikes/terminal.rs"
Write-Host ""

$failures = @()

if (-not $SkipIgnoredTest) {
    Write-Host "Running ConPTY integration test (cargo test --ignored)..." -ForegroundColor Cyan
    Push-Location $RepoRoot
    cargo test -p retcon-terminal powershell_is_interactive -- --ignored --nocapture
    $conptyExit = $LASTEXITCODE
    Pop-Location
    if ($conptyExit -ne 0) {
        $failures += "ConPTY ignored test (headless hosts often fail - retry on interactive Windows desktop)"
        Write-Host "ConPTY test did not pass in this session (exit $conptyExit)." -ForegroundColor Yellow
    } else {
        Write-Host "ConPTY test passed." -ForegroundColor Green
    }
}

function Test-ShellLaunch($Label, $Exe, [string[]]$ArgumentList) {
    Write-Host "Smoke: $Label" -ForegroundColor Cyan
    $p = Start-Process -FilePath $Exe -ArgumentList $ArgumentList -PassThru -WindowStyle Hidden
    Start-Sleep -Seconds 1
    if ($p.HasExited) {
        Write-Host "  $Label exited early (code $($p.ExitCode))" -ForegroundColor Yellow
    } else {
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        Write-Host "  $Label launched and was force-stopped." -ForegroundColor Green
    }
}

Test-ShellLaunch "PowerShell" "powershell.exe" @("-NoLogo", "-NoProfile", "-Command", "exit 0")
Test-ShellLaunch "Command Prompt" "cmd.exe" @("/c", "exit 0")
$bash = Get-Command bash.exe -ErrorAction SilentlyContinue
if ($bash) {
    Test-ShellLaunch "Git Bash" $bash.Source @("-lc", "exit 0")
} else {
    Write-Host "Git Bash not on PATH - install Git for Windows or add bash.exe to PATH." -ForegroundColor Yellow
    $failures += "Git Bash not found"
}

Write-Host ""
Write-Host "Interactive checklist (Retcon UI + real PTY):" -ForegroundColor Yellow
Write-Host "  - ANSI colors and Unicode (ok) in each shell profile"
Write-Host "  - Resize terminal panel; confirm reflow"
Write-Host "  - Normal exit vs force-stop; verify child-tree cleanup in Task Manager"

if ($failures.Count -gt 0) {
    Write-Host "`nFAIL ($($failures.Count) issue(s)):" -ForegroundColor Red
    $failures | ForEach-Object { Write-Host "  - $_" }
    Write-Host "FAIL"
    exit 1
}

Write-Host "`nPASS (automated smoke; complete UI checklist on desktop for full matrix sign-off)"
exit 0
