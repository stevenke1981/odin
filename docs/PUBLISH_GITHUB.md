# Publish the reviewed Odin candidate as a public GitHub repository

The approved public repository is [stevenke1981/odin](https://github.com/stevenke1981/odin). Actual publication, anonymous HTTP status and remote-head evidence are recorded in [publication.json](../verification/publication.json). The following procedure applies to later reviewed updates; local preparation alone is not publication evidence.

Odin is a Windows native preview with 72 tool contracts. The 40 requirements remain partially accepted; Linux/macOS, OS sandbox, broad language/IDE parity and benchmarks are incomplete. Publish that scope honestly. The completed local Windows checks, public source repository, packaged binary release and cloud CI are separate outcomes.

## Review before execution

Use the reviewed branch/worktree and commit only explicit intended paths. Complete the applicable local gates, preserve their command exit codes and source/binary hashes, run the requirement/manifest/publish-policy checks, and review the candidate for secrets and third-party licensing. The publisher does not stage or commit changes.

The scripts require a clean `main`, the approved GitHub login and matching origin. They refuse private repositories, an unverified existing remote, a mismatched expected SHA and force-push. They create a new public repository only when the scoped target is verified missing; otherwise they push to the matching public repository.

## Windows / PowerShell

```powershell
gh auth status
.\scripts\publish_public.ps1
$odinReviewedHead = (git rev-parse HEAD).Trim()
.\scripts\publish_public.ps1 -Execute -ExpectedHead $odinReviewedHead
```

The first publisher invocation is a dry-run of the publication step; it still performs local checks and reads the authenticated GitHub account. `-Execute` is the explicit mutation switch. The expected SHA must be the independently reviewed clean candidate, not a value obtained to bypass review.

## Linux/macOS

```bash
bash scripts/publish_public.sh
bash scripts/publish_public.sh --execute <reviewed-head-sha>
```

These commands are publishing procedures, not evidence that Odin has passed native Linux/macOS tests.

## Record completion accurately

After a successful operation, verify public visibility and `refs/heads/main` against the reviewed SHA, then update the delivery report with the actual URL and evidence. Do not infer a push from a ZIP, a successful local commit, a queued operation or a repository page alone. No credentials belong in the repository or reports.

The included workflow is `workflow_dispatch` plus trusted self-hosted only. The publisher does not dispatch CI, automatically stage/commit, force-push, merge a PR, create a binary release or alter repository visibility. Public forks must not run automatically on an unprotected self-hosted runner.
