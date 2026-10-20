# Explicit public publication of an already reviewed, clean commit. Never stages or commits files.
param([switch]$Execute, [string]$ExpectedHead = '', [string]$Repository = 'stevenke1981/odin')
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $Root
try {
    if ($Repository -ne 'stevenke1981/odin') { throw 'This publisher is scoped to stevenke1981/odin.' }
    foreach ($Name in @('git','gh','python')) { if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) { throw "Missing executable: $Name" } }
    $Head = (& git rev-parse --verify HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Commit the reviewed source before publication.' }
    if ((& git branch --show-current).Trim() -ne 'main') { throw 'Expected main branch; no automatic branch changes.' }
    if (& git status --porcelain) { throw 'Working tree must be clean. Review and commit explicit paths first.' }
    foreach ($Script in @('scripts/check_requirements.py','scripts/verify_manifest.py')) {
        & python $Script
        if ($LASTEXITCODE -ne 0) { throw "$Script failed: $LASTEXITCODE" }
    }
    $Login = (& gh api user --jq '.login').Trim()
    if ($LASTEXITCODE -ne 0 -or $Login -ne 'stevenke1981') { throw 'Authenticate the approved GitHub account stevenke1981.' }
    $Remotes = @(& git remote)
    if ($Remotes -contains 'origin') {
        $RemoteUrl = (& git remote get-url origin).Trim()
        if ($RemoteUrl -notmatch '^(?:https://github\.com/|git@github\.com:)stevenke1981/odin(?:\.git)?$') { throw 'Origin does not match the approved repository.' }
    }
    # These source workflows may only run by explicit manual self-hosted dispatch.
    & python scripts/check_publish_policy.py
    if ($LASTEXITCODE -ne 0) { throw 'Workflow publication policy failed.' }
    if (-not $Execute) { Write-Host "DRY_RUN: publish reviewed $Head to public https://github.com/$Repository. No remote changed. Use -Execute -ExpectedHead $Head for the authorized publication."; return }
    if ($ExpectedHead -ne $Head) { throw 'ExpectedHead must match the reviewed commit.' }
    $Visibility = & gh repo view $Repository --json isPrivate --jq '.isPrivate' 2>$null
    if ($LASTEXITCODE -eq 0) {
        if ($Visibility.Trim() -ne 'false') { throw 'Existing repository is private; no visibility changes are performed.' }
        if ($Remotes -notcontains 'origin') { & git remote add origin "https://github.com/$Repository.git"; if ($LASTEXITCODE -ne 0) { throw 'Could not add origin.' } }
        & git push -u origin HEAD:refs/heads/main
        if ($LASTEXITCODE -ne 0) { throw 'Push rejected; no force push performed.' }
    } else {
        if ($Remotes -contains 'origin') { throw 'Could not verify existing origin repository; resolve authentication/network/repository state explicitly.' }
        & gh repo create $Repository --public --description 'Independent Rust MCP code intelligence and journaled editing' --source . --remote origin --push
        if ($LASTEXITCODE -ne 0) { throw 'Repository creation or push failed.' }
    }
    $RemoteHead = ((& git ls-remote origin refs/heads/main) -split '\s+')[0]
    if ($LASTEXITCODE -ne 0 -or $RemoteHead -ne $Head) { throw 'Remote HEAD verification failed.' }
    $Visibility = (& gh repo view $Repository --json isPrivate --jq '.isPrivate').Trim()
    if ($LASTEXITCODE -ne 0 -or $Visibility -ne 'false') { throw 'Public visibility verification failed.' }
    Write-Host "Published and verified $Head at https://github.com/$Repository. No CI was dispatched."
} finally { Pop-Location }
