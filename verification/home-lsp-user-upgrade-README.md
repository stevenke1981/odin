External actual-user upgrade runner, prepared for root execution only after the
q7 isolated lifecycle/global state seal. Preparation does not authorize execution.

`user_upgrade.py --check-runner` is inert. It does not read actual user config,
start Odin, run the installer, take a certificate snapshot, or change grants.

The real `--execute` mode is pinned to final pkg7 installer/binary/setup/payload
manifest hashes in `PINS`. It requires freshly reviewed host/client SHA-256 values
and two explicit existing directories. Select two directories outside registered
project trees and protected storage, within existing allowed roots or actual Home.
For example, existing Home/Documents and Home/Downloads can be used after checking
their identities; no contents are requested. The runner does not create workspace
fixture folders, register projects, change the Codex entry, or restart the client.

Execution template (root must first replace both SHA values and both root paths):

    C:/Python314/python.exe C:/OdinQA/home-progress-1022/user-upgrade/user_upgrade.py --execute --expected-host-sha256 REVIEWED_SHA --expected-client-sha256 REVIEWED_SHA --workspace-a EXISTING_ABSOLUTE_ROOT_A --workspace-b EXISTING_ABSOLUTE_ROOT_B

The normal per-user installer is invoked without `/ODIN_QA` or profile replacement.
`PrivilegesRequired=lowest` in the frozen packaging/odin.iss establishes per-user
scope. `/CLOSEAPPLICATIONS /NOFORCECLOSEAPPLICATIONS /NORESTARTAPPLICATIONS` use
Inno Restart Manager; there are no broad process kills and PID21188 is not treated
as a test-owned process. The client must reconnect/restart after the upgrade;
success here does not mean its already-open session loaded the new binary.

Standard flags verified against the official Inno documentation:
https://jrsoftware.org/ishelp/topic_setupcmdline.htm

Custom `/ODIN_HOST_CONFIG`, `/ODIN_WORKSPACE_ROOT`, `/ODIN_SETUP_RECEIPT` arguments
are implemented by the current packaging/odin.iss. No
`/ODIN_EXPECTED_HOST_SHA256` argument exists. The runner checks the reviewed host
hash immediately before launching the installer. The worker subsequently anchors
its own plan CAS when it starts. These guards are separate, not an atomic binding
of the wrapper snapshot throughout installation. A receipt before-hash different
from the reviewed snapshot fails verification; conflicts are never forced.

Each real attempt creates a fresh external run directory and preserves first
failure, before host backup, host/client/Rust/grant hashes, user Path digest,
certificate counts/identity digests, installed file hashes, actual worker 19-probe
receipt, payload closure verification, 14 primary profiles/12 invocations, and
actual readonly CLI/MCP bindings. No private Home contents or raw client config
are saved or printed. `get_current_config` output is summarized and hashed rather
than retained; native `languages --project` independently provides the configured
14 profiles/12 invocations and is likewise retained only as hashes and summaries.
The existing configured Rust program's fixed `--version` is a separate actual
operator-preserved version-only check, not substituted bundled rust-analyzer or
semantic acceptance proof. No repository executable is run. Native read-only
tools can create normal private audit/state data.

Existing Rust profiles are required to remain identical for this specific upgrade.
Existing alternative provider choices cause a guarded failure rather than silently
resetting them to defaults. There is no retry, automatic rollback, grant editing,
or conflict overwrite. If interrupted/timed out, inspect the first attempt and
installer worker receipt before taking another action. The 1800-second installer
observer deadline records failure and leaves a still-running installer alone;
it does not force-kill a worker in its short CAS commit. Language semantics remain
NOT_RUN in this upgrade runner; use the separate source-bound real-backend reports.

Path identity accepts the existing Codex entry's canonical `\\?\C:\...` absolute
drive presentation as the same local drive path as `C:\...`. It does not strip
extended UNC, GLOBALROOT, device, drive-relative, dot-segment, trailing-dot/space,
or DOS-device paths. Original reparse-point ancestor checks remain unchanged.
The d15 runner and first safe prelaunch failure remain in history and the original
run directory; subsequent preparation evidence uses a fresh directory.
