# Optional native Windows OS sandbox

Odin's `odin-core::sandbox::prepare` builds an enforcing Microsoft eXecution
Container (MXC) ProcessContainer launch. It uses the host-selected native
`wxc-exec.exe` directly. Node and Python are not required by this runtime path.
MXC is an external host prerequisite; Odin does not download it, change Windows
features or ACLs, or accept telemetry terms on the host's behalf.

This module is optional. A host must explicitly configure `MxcSandboxConfig`.
An absent configuration means the existing process path is **unsandboxed**; a
configured sandbox that fails validation, probing, policy construction or launch
must fail the operation. Callers must never retry without the sandbox or enable
audit mode to make a workload run. Shell, debug and project-write permissions
remain separate host decisions; this process boundary does not enable them.

## Host configuration and protected storage

The host supplies an absolute executor path and its exact SHA-256, explicit
read-only, read-write and denied paths, a dedicated `temp_root`, and optionally
`allow_ui = true`. The default is `allow_ui = false`. Repository data and RPC
arguments cannot provide executable sandbox configuration.

`Config::load` canonicalizes host paths, checks the executor pin, and merges the
host configuration file and Odin state directory into denied paths. Runtime
`prepare` validates paths and the executable pin again and probes the installed
native backend before producing a command. The trusted caller supplies
`policy_dir`, under its host-controlled state, independently of repository input.
It must exist before preparation. Existing denied paths must also exist at
runtime so the final preflight can resolve them without guessing an alias.

Paths must be absolute local Windows paths without dot components, alternate
data streams, trailing dots/spaces, symbolic links or reparse components. Duplicate
case aliases are rejected. Writable grants cannot overlap denied paths, the
policy directory, read-only grants, or the scratch root, and cannot contain the
native executor. A broad read-only parent may contain a denied child; the native
deny rule is required and tested for that case. The workload executable and cwd
must be covered by explicit readable grants and must not be denied.

The scratch root cannot overlap the cwd or policy storage, or be an ancestor of
the user, system or existing host temporary directories. Every launch receives
only a newly created random child of this dedicated root as an additional writable
grant. The parent scratch root is not added as a writable grant. Policy JSON is
created with exclusive temporary-file creation in the protected policy directory;
that directory is always denied to the workload.

Validation and executor hashing protect against accidental configuration drift.
They do not replace trusted ownership of the executor, policy/state directories,
or the host configuration. A malicious same-user host process can race file
validation or edit an Odin binary; defending that host is outside this boundary.
Do not put credentials in granted locations or create credential hardlinks inside
a granted tree. A grant authorizes content reachable within that tree; this is
not an immutable filesystem snapshot or a VM.

## Enforced policy

The generated stable `1.0.0` policy fixes these choices:

- `containment = processcontainer`; the current probe must select
  `base-container`, report deny-path support and no DACL augmentation or warnings.
  Hosts exposing the BFS fallback are rejected pending separate verification.
- Egress and ingress defaults are `deny`, with ingress `hostLoopback = deny`.
- `fallback.allowDaclMutation = false`. There is no audit or alternate-tier retry.
- `lifecycle.destroyOnExit = true`, `preservePolicy = false`, and telemetry disabled.
- An explicit runtime environment with `inheritDefaultEnv = false`.
- UI disabled unless the host explicitly sets `allow_ui = true`; clipboard
  access remains `none` and input injection remains disabled in both cases.

Known runtime plumbing variables are allowlisted by `process::configure_child`,
including Go and .NET SDK/cache paths. Token variables such as `ODIN_TOKEN` and
`GITHUB_TOKEN` are not inherited. Per-child `GOTELEMETRY=off`,
`DOTNET_CLI_TELEMETRY_OPTOUT=1` and `DOTNET_GENERATE_ASPNET_CERTIFICATE=false` do not
change global host settings. Within a sandbox, `HOME`, `USERPROFILE`, `APPDATA`,
`LOCALAPPDATA`, `TEMP` and `TMP` point into the private scratch. Explicit host
`DOTNET_CLI_HOME`, Go cache and SDK paths are retained, but remain subject to the
filesystem grants; preserving an environment variable grants no file access.

The native backend may replace TEMP/TMP with a package-specific subdirectory of
that private scratch. On the tested executor it does not create that directory.
Workloads must initialize their own temporary directory, or fail; Odin does not
grant the original host temp directory or automatically loosen the policy. The
native workload fixture creates its own TEMP directory before the scratch-write
positive check. Initial failure logs preserving this SDK behavior are included.

Some Win32 runtimes require UI support even for a stdio server. Such a host must
make an explicit `allow_ui = true` decision. That configuration is not evidence
of complete UI isolation. Failure with UI disabled must not trigger a retry with
UI enabled. Required runtime/dependency directories must likewise be explicitly
granted by the host; executable discovery never grants directories automatically.

## Launch and cancellation ABI

`prepare(config, program, args, cwd, policy_dir, timeout)` returns
`SandboxLaunch { command, guard }`. Arguments are passed as native argv after
`--config <policy> -- <executable>` without invoking a host shell. The native CLI
leaves workload stdin, stdout and stderr as byte streams, allowing JSON-RPC
Content-Length framing and binary payloads to pass unchanged.

The caller chooses the stdio pipes, converts the command to Tokio if needed,
enables kill-on-drop, and attaches `process::ProcessTree` immediately after spawn.
It must retain the `Send + Sync` `SandboxGuard` for the entire child and descendant
lifetime, then reap the child before dropping the guard. Dropping it removes the
temporary policy and private scratch. A launch failure is an error, never a
request to execute the original command directly.

MXC confines the workload before it starts. The outer Windows job used by
`ProcessTree` provides cancellation and descendant cleanup; job assignment itself
does not establish filesystem/network isolation. `None` means no native timeout;
an explicit timeout below one millisecond is rejected, so it cannot silently
become MXC's zero/unlimited setting. Native launch/timeout exit codes are preserved.

## Verification and limits

The opt-in Windows integration test compiles a small pure-Rust workload with the
installed Rust toolchain. It then runs the actual pinned native executor, with
no mocked backend. Set the trusted executor path and pin explicitly before running:

```powershell
$env:ODIN_MXC_EXECUTOR = '<absolute host path to wxc-exec.exe>'
$env:ODIN_MXC_SHA256 = '<verified SHA-256>'
cargo test -p odin-core --test os_sandbox_native --locked -- --ignored --test-threads=1 --nocapture
```

Executed acceptance covers exact binary stdin/stdout and stderr, permitted
read/write and private scratch, denied reads/writes nested within read-only
grants, read-only overwrite/create denial, outside reads/writes/executable launch,
junction escape denial, filtered environment, direct and descendant loopback
denial with zero host accepts, nonzero child exit propagation, invalid pin/path
rejection, native executable-launch failure, and parent/descendant termination
after cancellation and native timeout. It checks stopped heartbeats and actual
process disappearance, plus policy/scratch cleanup after a normal exit.

Command logs, source hashes and scope are recorded in
`verification/sandbox-verification.json`. This module's native tests establish
the launch boundary; integrated Odin MCP/LSP/DAP/shell verification belongs to
the runtime owners and must be reported separately. Fixtures are not real
language-server or debugger evidence. Linux/macOS, BFS/AppContainer fallback,
VM backends, ingress server behavior, external internet destinations, registry
and all Windows system resources are not exhaustively tested by this suite.
Windows exposes some OS/runtime resources independently of user path grants;
these tests prove the named application-path negative cases, not a claim that
every system DLL or device is inaccessible. Full UI isolation and resistance
to a compromised host/kernel remain unverified.

Current upstream contracts used for this implementation:
[native executor and enforcement modes](https://github.com/microsoft/mxc/blob/main/README.md),
[configuration semantics](https://github.com/microsoft/mxc/blob/main/docs/schema.md),
[stable policy schema](https://github.com/microsoft/mxc/blob/main/schemas/stable/mxc-config.schema.1.0.0.json),
and [ProcessContainer OS support](https://github.com/microsoft/mxc/blob/main/docs/backends/process-container/os-version-support.md).
