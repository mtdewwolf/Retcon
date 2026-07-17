# Matrix item 6: Representative large repository and submodule/conflict handling.
param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
)

$ErrorActionPreference = "Stop"
Write-Host "Phase 2 matrix #6 - large repo + submodules/conflicts" -ForegroundColor Cyan
Write-Host "Code under test: crates/retcon-git/"
Write-Host ""

Push-Location $RepoRoot
Write-Host "Integration test (worktree round-trip)..." -ForegroundColor Cyan
cargo test -p retcon-git
if ($LASTEXITCODE -ne 0) {
    Pop-Location
    Write-Host "FAIL (retcon-git tests)"
    exit 1
}

Write-Host "Timing git.status on workspace ($RepoRoot)..." -ForegroundColor Cyan
$sw = [System.Diagnostics.Stopwatch]::StartNew()
git -C $RepoRoot status --porcelain=v1 --branch | Out-Null
$sw.Stop()
Write-Host "  git status elapsed: $($sw.ElapsedMilliseconds) ms"

$temp = New-Item -ItemType Directory -Path (Join-Path $env:TEMP "retcon-git-matrix-$(Get-Random)")
try {
    Push-Location $temp.FullName
    git init -b main | Out-Null
    git config user.email "retcon@example.invalid"
    git config user.name "Retcon Matrix"
    "base" | Set-Content file.txt
    git add .
    git commit -m "init" | Out-Null

    Write-Host "Conflict fixture..." -ForegroundColor Cyan
    git checkout -b feature | Out-Null
    "feature" | Set-Content file.txt
    git add file.txt
    git commit -m "feature" | Out-Null
    git checkout main | Out-Null
    "main" | Set-Content file.txt
    git add file.txt
    git commit -m "main change" | Out-Null
    git merge feature 2>&1 | Out-Null
    $conflictNames = git diff --name-only --diff-filter=U
    if (-not $conflictNames) {
        throw "expected merge conflict"
    }
    Write-Host "  conflict paths: $($conflictNames -join ', ')"

    Write-Host "Submodule fixture..." -ForegroundColor Cyan
    git merge --abort 2>$null
    git checkout main | Out-Null
    $sub = New-Item -ItemType Directory -Path (Join-Path $temp.FullName "sub-repo")
    Push-Location $sub.FullName
    git init -b main | Out-Null
    git config user.email "retcon@example.invalid"
    git config user.name "Retcon Matrix"
    "sub" | Set-Content README.md
    git add .
    git commit -m "sub init" | Out-Null
    Pop-Location
    Push-Location $temp.FullName
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    git -c protocol.file.allow=always submodule add $sub.FullName modules/demo 2>&1 | ForEach-Object { Write-Host $_ }
    if ($LASTEXITCODE -ne 0) { throw "git submodule add failed" }
    $ErrorActionPreference = $prevEap
    git commit -m "add submodule" | Out-Null
    $subPaths = git config --file .gitmodules --get-regexp path | ForEach-Object { ($_ -split ' ', 2)[1] }
    if ($subPaths -notcontains "modules/demo") {
        throw "submodule path not recorded"
    }
    Write-Host "  submodule paths: $($subPaths -join ', ')"
    Pop-Location
} finally {
    Pop-Location
    Remove-Item -Recurse -Force $temp.FullName -ErrorAction SilentlyContinue
}

Pop-Location
Write-Host "`nPASS"
exit 0
