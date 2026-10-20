#!/usr/bin/env bash
# Explicitly publish a reviewed clean commit. Does not stage, commit, force-push or dispatch CI.
set -euo pipefail
cd "$(dirname "$0")/.."
for cmd in git gh python3; do command -v "$cmd" >/dev/null || { echo "Missing $cmd" >&2; exit 1; }; done
repo='stevenke1981/odin'
head="$(git rev-parse --verify HEAD)"
[[ "$(git branch --show-current)" == main ]] || { echo 'Expected main branch' >&2; exit 1; }
[[ -z "$(git status --porcelain)" ]] || { echo 'Working tree must be clean; review and commit explicit paths' >&2; exit 1; }
python3 scripts/check_requirements.py
python3 scripts/verify_manifest.py
python3 scripts/check_publish_policy.py
[[ "$(gh api user --jq '.login')" == stevenke1981 ]] || { echo 'Expected approved GitHub account stevenke1981' >&2; exit 1; }
if git remote get-url origin >/dev/null 2>&1; then
  url="$(git remote get-url origin)"
  case "$url" in https://github.com/stevenke1981/odin|https://github.com/stevenke1981/odin.git|git@github.com:stevenke1981/odin|git@github.com:stevenke1981/odin.git) ;; *) echo 'Origin does not match approved repository' >&2; exit 1;; esac
fi
if [[ "${1:-}" != --execute ]]; then
  printf 'DRY_RUN: public https://github.com/%s at %s. To publish, pass --execute %s\n' "$repo" "$head" "$head"
  exit 0
fi
[[ "${2:-}" == "$head" ]] || { echo 'Expected SHA does not match reviewed HEAD' >&2; exit 1; }
if visibility="$(gh repo view "$repo" --json isPrivate --jq '.isPrivate' 2>/dev/null)"; then
  [[ "$visibility" == false ]] || { echo 'Private repository; refusing visibility change' >&2; exit 1; }
  git remote get-url origin >/dev/null 2>&1 || git remote add origin "https://github.com/${repo}.git"
  git push -u origin HEAD:refs/heads/main
else
  if git remote get-url origin >/dev/null 2>&1; then echo 'Could not verify existing remote; resolve state explicitly' >&2; exit 1; fi
  gh repo create "$repo" --public --description 'Independent Rust MCP code intelligence and journaled editing' --source . --remote origin --push
fi
[[ "$(git ls-remote origin refs/heads/main | cut -f1)" == "$head" ]] || { echo 'Remote HEAD verification failed' >&2; exit 1; }
[[ "$(gh repo view "$repo" --json isPrivate --jq '.isPrivate')" == false ]] || { echo 'Public visibility verification failed' >&2; exit 1; }
printf 'Published and verified %s at https://github.com/%s. No CI dispatched.\n' "$head" "$repo"
