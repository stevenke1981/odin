# Publish Odin as a new **public** GitHub repository

## Publishing status of this delivery

At the time of authoring, the connected account is `stevenke1981`, but `stevenke1981/odin` returns HTTP 404 (repository not found). The available ChatGPT GitHub connector has `create_blob`, `create_tree`, `create_commit`, `create_file`, and `update_ref`, but **does not expose a create-repository API**. We cannot infer that a ZIP upload constitutes a GitHub push. Therefore this package is **NOT_PUBLISHED** until a user with the required GitHub permission creates the public repository and pushes the files.

Once an empty public repository exists, the connected GitHub integration can perform a tree/commit push. Alternatively, run the included publisher from a GitHub-authenticated local PC.

## Windows / PowerShell (preferred on 5900X)

1. Extract the ZIP and open PowerShell **in the folder containing `Cargo.toml`, `README.md`, `requirements/`**.
2. Run `gh auth status` first. Only a GitHub account with write access to `stevenke1981` can create/push the repo.
3. Run: `powershell -ExecutionPolicy Bypass -File .\scripts\publish_public.ps1`
4. Check [the public repository](https://github.com/stevenke1981/odin) and [the feature specification](https://github.com/stevenke1981/odin/blob/main/docs/FEATURE_SPEC.md) after success.

The script will locally verify the 40 requirements/53 tool mappings, run manifest checks, stage, commit, create `stevenke1981/odin` **public** if missing, and push to `main`. If the repository already exists, it refuses to force-push or publish into a private repo. It does not create a release, dispatch CI, install dependencies, or charge for GitHub-hosted runners.

## Linux/macOS

Run `bash scripts/publish_public.sh` with `git`, `gh`, and `python3` available.

## Security and honesty

No GitHub credentials are included in this archive. Secret scanning and an explicit file review are recommended before public publication. The included `.github/workflows/manual-local.yml` is **manual dispatch + self-hosted only** and cannot run merely because you push the source; do not grant untrusted forks permission to trigger self-hosted runners. Rust compilation and true LSP/DAP integration remain NOT_RUN.
