# Windows LSP payload builder

`prepare_lsp_payload.py` builds the external `backends` directory used by the Windows installer. It requires Windows x64 and Python 3.11 or later. It does not install Odin, edit PATH, register MCP servers, or certify IDE support.

Run from a host-controlled checkout, with a fresh output directory outside every Git checkout:

```powershell
python -B scripts/prepare_lsp_payload.py --output C:/OdinBuild/release/backends --cache-dir C:/OdinBuild/archive-cache --versions
```

The output directory must not exist, even if empty. The default create-only receipt is `C:/OdinBuild/release/backends.build-receipt.json`. `--receipt` selects another external path outside the payload and cache. A failed build retains its partial output, logs, caches, and FAIL receipt; use a different fresh output and receipt after investigating. The builder never replaces a cache hash conflict.

`--package-json` and `--package-lock` can select the checked-in `packaging/lsp-package.json` and `packaging/lsp-package-lock.json` from another checkout. The complete lockfile bytes must match SHA256 `58cb6e0a67c4283bc56e29c4cd89a4304bf587bbbba92fe4d38024d35082b4ba`. Package metadata is parsed as data. The exact direct dependency versions, registry HTTPS URLs, and SHA512 integrity values are checked; no repository lifecycle scripts run.

The fixed original archives may be placed in the cache before the build:

- `node-v24.21.0-win-x64.zip`
- `lua-language-server-3.19.2-win32-x64.zip`
- `go1.27.3.windows-amd64.zip`
- `dotnet-sdk-10.0.402-win-x64.zip`
- `csharp-ls.0.28.0.nupkg`
- `csharp-ls-0.28.0-LICENSE`

Missing originals are downloaded only from the fixed official HTTPS sources recorded in the script. Every reuse checks its pinned SHA256, or the official SHA512 for .NET. ZIP archives are checked again through the same open handle used for extraction. The entire ZIP directory is preflighted before writing: traversal, Windows device names, streams, case collisions, file ancestors, links, reparse attributes, encrypted entries, and oversized archives are rejected. Output, cache and receipt ancestors must not be symlinks or junctions. These checks assume host-owned build directories and do not sandbox a concurrently hostile local process.

The payload contains the complete Node 24.21.0, Go 1.27.3, .NET SDK 10.0.402 and LuaLS 3.19.2 distributions. `csharp/` contains the pinned csharp-ls 0.28.0 `tools/net10.0/any` closure. Bundled Node runs `npm ci --ignore-scripts --no-audit --no-fund` with fresh external cache and empty user/global npm configuration. It installs basedpyright 1.40.2, typescript-language-server 6.0.2, TypeScript 6.0.3, bash-language-server 5.8.1 smol-toml 1.9.1, yaml-language-server 2.0.0 and @vtsls/language-server 0.3.0. The pinned Go SDK builds gopls v0.24.0 with `-trimpath`, `GOTOOLCHAIN=local`, `GOSUMDB=sum.golang.org`, `GOPROXY=https://proxy.golang.org`, `GOENV=off`, and `CGO_ENABLED=0`; module caches and `GOBIN` are explicit external paths. No QA runtime directories are copied. The controlled .NET environment fixes `DOTNET_ADD_GLOBAL_TOOLS_TO_PATH=false` and `DOTNET_GENERATE_ASPNET_CERTIFICATE=false`; an external CLI home alone does not disable SDK first-use changes to persistent user settings.

The build uses fresh sibling work directories. Exact process argv, working directory, controlled environment, exit codes, elapsed times, and stdout/stderr log hashes are recorded in the receipt. The Go binary build information is an actual recorded command. `--versions` additionally executes version probes with Lua log/meta paths outside the payload and compares every payload file before/after to reject probe mutations; semantic operations, project loading, diagnostics, references and rename still require the real MCP integration fixtures. A manifest covers every payload file except itself with sorted relative paths and SHA256; hashing uses a bounded pool of 16 workers.

Original SDK and npm notice files remain in their distributions. `licenses/npm-packages.json` inventories the locked npm closure. An absent package is recorded as `NOT_INSTALLED_PLATFORM_OPTIONAL` only when the lock marks it optional and its explicit `os` or `cpu` metadata excludes Windows x64 (for example macOS-only fsevents); missing applicable packages or partial package directories fail the build. Go module LICENSE/NOTICE/COPYING/COPYRIGHT/PATENTS files are retained under `licenses/go-modules/`. The NuGet package does not supply a LICENSE member, so the build includes the csharp-ls MIT LICENSE from immutable upstream commit `72671fc7fa810c158864e72485db68e8b9b25c3d`, pinned SHA256 `2077a187c02688a6e2e816ea81f846667c50d09c2ee5c7f0f82594b452e63118`. This preserves supplied notices and provenance; it is not a complete legal review of every transitive assembly.

Reproducible means pinned, verified inputs and a recorded build recipe. Network access is required for uncached original archives, fresh npm packages and Go modules. Byte-identical rebuilds across arbitrary hosts are not yet verified. The script is a developer build tool, not a security sandbox or an installer. A standalone rust-analyzer is bundled; the Rust toolchain remains host-managed and operator Rust profiles are retained.

Offline regression command:

```powershell
python -B -m unittest discover -s tests -p test_lsp_payload_builder.py -v
```

These tests exercise safe extraction, cache preservation, output boundaries, manifest completeness, lockfile rejection and real nonzero command logging. They do not download, install or build a backend; real payload build and MCP acceptance remain separate evidence.

## Expanded native assets and notice sources

`packaging/lsp-native-sources.json` pins Slint LSP, clangd, Taplo, Marksman, Ruff, ty and rust-analyzer assets, entry points, source commits and license texts. Its exact SHA256 is checked by the builder. Native ZIPs use the same strict preflight. Only the pinned Slint ZIP permits normalization of Windows path separators; traversal, absolute/device names and case collisions remain rejected. `--versions` now runs 19 executable version probes; the manifest has 20 components because the notice/source inventory is data, not another executable probe.

`collect_lsp_notices.py` is a pinned developer collector. It parses Cargo locks and Marksman's actual embedded NuGet metadata as data, downloads fixed archives, and retains hashes/provenance and declared missing license texts. Cargo inventories include dev and non-Windows packages; they are lock supersets, not exact linked SBOMs. Collection never executes repository build scripts or treats package text as host configuration. The status is `WITH_DECLARED_NOTICE_GAPS`: Taplo 6, Ruff 9, ty 9, rust-analyzer 7 and Marksman 3 supplemental text gaps (component supersets can overlap). No replacement copyright attribution is invented.

Slint LSP is distributed as a separate GPL-3.0-only executable. A compressed corresponding-source sidecar includes the pinned Slint tree, Skia, 993 crate archives and 14 native dependency archives, with 1,009 inner source hashes. This sidecar stays compressed during installation. It contains a source/rebuild inventory, not an executed rebuild: actual reproduction of the official binary is `NOT_RUN`. Odin does not link Slint into its Rust service or change its own source license. See [expanded support and limits](EXPANDED_LSP.md) and the payload's `licenses/` and `sources/` indexes before redistribution.

The final prepared payload has 35,153 manifest-covered files. File and version counts identify that payload only; a new build must derive and verify its own manifest. The build receipt records four development commands plus 19 version probes; a successful payload build is separate from Odin release gates and installed semantic/installer acceptance.
