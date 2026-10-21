#!/bin/sh
set -u
export PATH="/opt/odin-rust-1.90/bin:$PATH"
export CARGO_HOME=/home/odin-qa/.cargo
export CARGO_TARGET_DIR=/root/odin-linux-target
export CARGO_BUILD_JOBS=4
export CC_x86_64_unknown_linux_musl=musl-gcc
export AR_x86_64_unknown_linux_musl=ar
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=cc
export ODIN_PYTHON=/usr/bin/python3
export CARGO_NET_OFFLINE=true
export RUST_TEST_THREADS=1
delivery=/opt/odin-delivery/v3-69e769b
test "$(cat "$delivery/build.exit")" = 0 || exit 95
test "$(cat "$delivery/metadata.exit")" = 0 || exit 95
cd /opt/odin-final || exit 98
test "$(git rev-parse HEAD)" = 69e769be9baa4e825139b72ce1a9b48bf8143114 || exit 97
test -z "$(git status --porcelain --untracked-files=no)" || exit 97
run_stage() {
  stage=$1
  shift
  start=$(date +%s)
  printf '%s\n' "$*" > "$delivery/$stage.command"
  "$@" > "$delivery/$stage.log" 2>&1
  result=$?
  printf '%s\n' "$result" > "$delivery/$stage.exit"
  printf '%s\n' "$(($(date +%s)-start))" > "$delivery/$stage.seconds"
  return "$result"
}
run_stage generated python3 scripts/check_generated.py || exit $?
run_stage static-source python3 scripts/check_source.py --output "$delivery/static-source-report.json" || exit $?
run_stage fixture python3 scripts/fixture_selftest.py --output "$delivery/fixture-report.json" || exit $?
run_stage requirements python3 scripts/check_requirements.py || exit $?
run_stage publish-policy python3 scripts/check_publish_policy.py || exit $?
run_stage cli python3 scripts/cli_smoke.py --binary "$delivery/odin" --output "$delivery/cli-report.json" || exit $?
run_stage mcp python3 scripts/mcp_smoke.py --binary "$delivery/odin" --output "$delivery/mcp-report.json" || exit $?
run_stage http python3 scripts/http_smoke.py --binary "$delivery/odin" --output "$delivery/http-report.json" || exit $?
run_stage cancellation python3 scripts/mcp_cancel_smoke.py --binary "$delivery/odin" --output "$delivery/cancellation-report.json" || exit $?
run_stage package bash scripts/package.sh dist/linux-candidate || exit $?
run_stage checksums sh -c 'cd /opt/odin-final/dist/linux-candidate && sha256sum --check SHA256SUMS' || exit $?
python3 - <<'PY'
import hashlib, json, pathlib, subprocess
delivery = pathlib.Path('/opt/odin-delivery/v3-69e769b')
binary = delivery/'odin'
report = json.loads(pathlib.Path('/opt/odin-final/dist/linux-candidate/package-report.json').read_text())
assert report['binary_sha256'] == hashlib.sha256(binary.read_bytes()).hexdigest()
assert report['build']['revision'] == '69e769be9baa4e825139b72ce1a9b48bf8143114'
assert report['build']['tracked_source_dirty'] is False
manifest = json.loads(pathlib.Path('/opt/odin-transfer/odin-69e769b-source-sha256.json').read_text())
root = pathlib.Path('/opt/odin-final')
for row in manifest['files']:
    content = (root/row['path']).read_bytes()
    assert hashlib.sha256(content).hexdigest() == row['sha256'], row['path']
assert not subprocess.check_output(['git','status','--porcelain','--untracked-files=no'])
(delivery/'source-after-qa.json').write_text(json.dumps({'status':'PASS','revision':manifest['revision'],'checked_files':len(manifest['files']),'clean_source':True,'binary_sha256':report['binary_sha256']},indent=2)+'\n')
PY
exit $?
