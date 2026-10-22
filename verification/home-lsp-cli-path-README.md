External current-user CLI PATH helper. Root may execute it only after the actual
user-upgrade runner seals. Preparation runs `--check-fixtures` and syntax parsing;
it does not snapshot/write registry, run Odin, or change any process/user PATH.

After that seal, root first runs:

    C:/Python314/python.exe C:/OdinQA/home-progress-1022/user-cli-path/user_cli_path.py --snapshot

The read-only snapshot pins actual physical OS Home, installed Odin binary SHA
2e1c11b040ab1c13e4843a0de7b912b0f8b9452d2284a5c353077df32e021792,
raw HKCU Path value and RegistryValueKind, and machine Path baseline. Raw values
are stored only in a private directory whose protected ACL permits the current
user and SYSTEM. Public reports contain SHA-256 digests, kinds, separator counts,
checks and the sealed installed application path, never the raw PATH value.

Root then supplies the exact snapshot path and SHA printed by that operation:

    C:/Python314/python.exe C:/OdinQA/home-progress-1022/user-cli-path/user_cli_path.py --apply --snapshot-path PRIVATE_SNAPSHOT_FILE --expected-snapshot-sha256 REVIEWED_SHA

Apply checks the snapshot SHA/source hashes/Home/binary, constructs the candidate
User PATH, and starts a fixed read-only child verifier before writing. If the
installed directory is not first, the only change is `installed_dir + ';' + old_raw`.
An existing later occurrence is retained as part of the exact old suffix; no old
entry is removed/reordered, including empty entries. If already first, it is a
no-op. Existing REG_SZ/REG_EXPAND_SZ kind is retained. Absent/other value kinds
fail without choosing a new kind. REG_EXPAND_SZ `%PATH%` self-reference is rejected
instead of using the current process's inherited PATH as an expansion source.

The final raw value and kind are immediately rechecked through the opened HKCU
key before its single SetValueEx. This expected-value guard is a separate registry
read/write, not atomic CAS against another process. A detected conflict stops;
there is no retry or rollback that could overwrite a concurrent actor.

Machine PATH is read-only. Both verifier processes receive a PATH built from the
observed persisted Machine PATH followed by candidate/actual persisted User PATH,
expanding REG_EXPAND_SZ with OS non-PATH environment variables. The prewrite probe
fails if a machine entry shadows the installed Odin; machine PATH is never edited.
The postwrite fresh child must prove Get-Command resolves the sealed installed
Odin, verify its SHA, and run only its fixed `metadata` command. No repository
commands, native config, feature grants, legacy binaries, or client entry change.

Existing process PATH is unchanged, no client restart is claimed, and there is no
environment broadcast in this helper. Fresh verification passes an explicit PATH;
other launchers may require restart/sign-in to inherit persisted changes. All
attempts preserve first failure and create new external reports/backups.

Private ACL handling validates the current owner and the exact protected two-rule
current-user/SYSTEM FullControl policy before doing anything. An already-correct
directory is a no-op. A current-user-owned directory needing restriction receives
only a DACL update through .NET; the helper does not change ownership or SACL,
request SeSecurityPrivilege, or relax the policy. ACL subprocess argv/exit/stdout/
stderr are retained create-only in acl-commands even when validation fails.
Changing helper source invalidates an earlier source-pinned snapshot; root must
take a fresh snapshot and pass its new SHA, retaining the earlier evidence.
