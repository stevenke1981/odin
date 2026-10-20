#!/usr/bin/env bash
# Explicit, no force: publish Odin standalone to stevenke1981/odin as public.
set -euo pipefail
cd "$(dirname "$0")/.."
for cmd in git gh python3; do command -v "$cmd" >/dev/null || { echo "Missing $cmd" >&2; exit 1; }; done
python3 scripts/check_requirements.py
python3 scripts/verify_manifest.py
gh auth status
if [ ! -d .git ]; then git init -b main; fi
[ "$(git branch --show-current)" = main ] || { echo 'Expected branch main, refusing push' >&2; exit 1; }
git add -A
if ! git diff --cached --quiet; then git commit -m 'docs: define Odin standalone requirements and acceptance criteria'; fi
repo='stevenke1981/odin'
if gh repo view "$repo" --json isPrivate --jq '.isPrivate' >/dev/null 2>&1; then
  [ "$(gh repo view "$repo" --json isPrivate --jq '.isPrivate')" = false ] || { echo 'Repo is private; will not change visibility' >&2; exit 1; }
  if ! git remote get-url origin >/dev/null 2>&1; then git remote add origin "https://github.com/${repo}.git"; fi
  url="$(git remote get-url origin)"
  case "$url" in https://github.com/stevenke1981/odin|https://github.com/stevenke1981/odin.git|git@github.com:stevenke1981/odin|git@github.com:stevenke1981/odin.git) ;; *) echo "Unexpected origin $url" >&2; exit 1;; esac
  git push -u origin main
else
  gh repo create "$repo" --public --description 'Odin standalone Rust MCP code intelligence engine' --source . --remote origin --push
fi
printf 'Published: https://github.com/%s\n' "$repo"
