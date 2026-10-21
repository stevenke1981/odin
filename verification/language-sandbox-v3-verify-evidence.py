"""Offline evidence integrity only; never launches Odin or a language backend."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_history(path):
    revisions = subprocess.check_output(["git", "log", "--format=%H", "--", path], text=True).splitlines()
    result = {}
    for revision in revisions:
        content = subprocess.check_output(["git", "show", f"{revision}:{path}"])
        result.setdefault(hashlib.sha256(content).hexdigest(), revision)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Preserve the previous index and choose a fresh output")
    histories = {name: source_history(name) for name in (
        "tests/real_ide_backends.py", "verification/language-sandbox-v3-runner.ps1",
        "verification/language-sandbox-v3-direct-diagnostic.py")}
    records = []
    for cp in sorted(Path("verification").glob("language-sandbox-v3-*.command.json")):
        stem = cp.name.removesuffix(".command.json")
        rp, lp = cp.parent / (stem + ".json"), cp.parent / (stem + ".txt")
        c, r = json.loads(cp.read_text(encoding="utf-8")), json.loads(rp.read_text(encoding="utf-8"))
        assert c["report_sha256"] == sha(rp), str(rp)
        assert c.get("log_sha256", c.get("stdout_sha256")) == sha(lp), str(lp)
        assert c["harness_sha256"] in histories["tests/real_ide_backends.py"], str(cp)
        source_bindings = {"harness": histories["tests/real_ide_backends.py"][c["harness_sha256"]]}
        if "driver_sha256" in c:
            assert c["driver_sha256"] == r["source_sha256"], str(cp)
            source_bindings["direct_driver"] = histories["verification/language-sandbox-v3-direct-diagnostic.py"][c["driver_sha256"]]
        if "runner_sha256" in c:
            source_bindings["runner"] = histories["verification/language-sandbox-v3-runner.ps1"][c["runner_sha256"]]
        artifacts = [rp, lp, cp]
        if "transcript_sha256" in r:
            tp = cp.parent / (stem + ".requests.jsonl")
            assert r["transcript_sha256"] == sha(tp), str(tp)
            artifacts.append(tp)
            assert c["harness_sha256"] == r["harness_sha256"] and c["binary_sha256"] == r["binary_sha256"]
        normalization = cp.parent / (stem + ".normalization.json")
        if normalization.is_file():
            n = json.loads(normalization.read_text(encoding="utf-8"))
            assert n["logical_checks_sha256_before"] == n["logical_checks_sha256_after"]
            for artifact in n["artifacts"]:
                path = cp.parent / Path(artifact["path"].replace("\\", "/")).name
                assert sha(path) == artifact["sha256"], str(path)
            artifacts.append(normalization)
        assert all(b"\r" not in path.read_bytes() for path in artifacts), str(cp)
        for manifest in r.get("private_python_asset_manifests", []):
            expected = hashlib.sha256(json.dumps(manifest["files"], sort_keys=True, separators=(",", ":")).encode()).hexdigest()
            assert expected == manifest["manifest_sha256"] and len(manifest["files"]) == 5427
        records.append({"receipt": cp.as_posix(), "actual_driver_exit_code": c["exit_code"],
            "report_status": r["status"], "scope": r["scope"], "binary_sha256": r.get("binary_sha256"),
            "native_backend_exit_code": r.get("native_exit_code"), "counts": r.get("counts"),
            "source_commit_bindings": source_bindings,
            "artifacts": [{"path": path.as_posix(), "sha256": sha(path)} for path in artifacts]})
    result = {"scope": "OFFLINE_EVIDENCE_INTEGRITY_ONLY", "status": "PASS", "backend_executions": 0,
        "historical_results_modified": False, "command": [sys.executable, *sys.argv],
        "source_sha256": sha(Path(__file__)), "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "verified_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "line_endings": "LF",
        "dependency_sha256": {"tests/protocol_harness.py": sha(Path("tests/protocol_harness.py"))},
        "receipts_verified": len(records), "records": records}
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps({"status": "PASS", "scope": result["scope"], "receipts_verified": len(records), "index_sha256": sha(args.output)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
