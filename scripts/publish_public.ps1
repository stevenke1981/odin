# Publish this exact Odin checkout to the owner's new public GitHub repo.
# Must be run explicitly on a local machine with authenticated gh CLI.
param([string]$Repository = 'stevenke1981/odin')
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $Root
try {
    if ($Repository -ne 'stevenke1981/odin') { throw 'Only stevenke1981/odin is approved by this publisher.' }
    foreach ($exe in @('git','gh','python')) {
        if (-not (Get-Command $exe -ErrorAction SilentlyContinue)) { throw "Missing executable: $exe" }
    }
    & python scripts/check_requirements.py
    if ($LASTEXITCODE -ne 0) { throw 'Spec traceability check failed' }
    & python scripts/verify_manifest.py
    if ($LASTEXITCODE -ne 0) { throw 'Manifest integrity check failed' }
    & gh auth status
    if ($LASTEXITCODE -ne 0) { throw 'Sign in with gh auth login first' }
    if (-not (Test-Path (Join-Path $Root '.git'))) {
        & git init -b main
        if ($LASTEXITCODE -ne 0) { throw 'git init failed' }
    }
    $branch = (& git branch --show-current).Trim()
    if ($branch -ne 'main') { throw "Expected main branch, got $branch. Refusing unexpected push." }
    & git add --all
    if ($LASTEXITCODE -ne 0) { throw 'git add failed' }
    & git diff --cached --quiet
    $hasChanges = $LASTEXITCODE -eq 1
    if ($LASTEXITCODE -gt 1) { throw 'git diff failed' }
    if ($hasChanges) {
        & git commit -m 'docs: define Odin standalone requirements and acceptance criteria'
        if ($LASTEXITCODE -ne 0) { throw 'git commit failed; configure your git user.name and user.email' }
    }
    & gh repo view $Repository --json isPrivate --jq '.isPrivate' 2>$null
    $found = $LASTEXITCODE -eq 0
    if ($found) {
        $visibility = (& gh repo view $Repository --json isPrivate --jq '.isPrivate').Trim()
        if ($visibility -ne 'false') { throw 'Repo is private. No automatic visibility changes.' }
        $remotes = @(& git remote)
        if ($remotes -notcontains 'origin') { & git remote add origin "https://github.com/$Repository.git" }
        $remoteUrl = (& git remote get-url origin).Trim()
        if ($remoteUrl -notmatch 'github\.com[:/]stevenke1981/odin(?:\.git)?$') { throw "Unexpected origin URL: $remoteUrl" }
        & git push -u origin main
        if ($LASTEXITCODE -ne 0) { throw 'Push rejected; no force push performed' }
    } else {
        & gh repo create $Repository --public --description 'Odin standalone Rust MCP code intelligence engine' --source . --remote origin --push
        if ($LASTEXITCODE -ne 0) { throw 'Could not create public repo or push it' }
    }
    Write-Host "Published: https://github.com/$Repository"
} finally { Pop-Location }
