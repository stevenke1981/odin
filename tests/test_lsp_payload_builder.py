"""Offline builder regression checks; these never build or install LSPs."""
from __future__ import annotations

import base64
from dataclasses import replace
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

SOURCE = Path(__file__).resolve().parents[1] / "scripts/prepare_lsp_payload.py"
SPEC = importlib.util.spec_from_file_location("odin_payload_builder", SOURCE)
assert SPEC is not None and SPEC.loader is not None
builder = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = builder
SPEC.loader.exec_module(builder)


class BuilderTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="odin-builder-tests-")
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)

    def archive(self, entries):
        path = self.root / "input.zip"
        with zipfile.ZipFile(path, "w") as archive:
            for entry, content in entries:
                if isinstance(entry, str):
                    # ZipInfo normally normalizes backslashes on Windows; retain
                    # hostile raw names in the central directory for this fixture.
                    info = zipfile.ZipInfo("placeholder")
                    info.filename = info.orig_filename = entry
                    entry = info
                archive.writestr(entry, content)
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        artifact = builder.Artifact("test", "1", path.name, "https://nodejs.org/test.zip", "sha256", digest)
        return path, artifact

    def test_safe_archive_strips_exact_prefix(self):
        path, artifact = self.archive([("sdk/", b""), ("sdk/bin/server.exe", b"binary"), ("notice.txt", b"notice")])
        output = self.root / "payload"
        builder.extract_zip(path, output, artifact, "sdk/")
        self.assertEqual((output / "bin/server.exe").read_bytes(), b"binary")
        self.assertFalse((output / "notice.txt").exists())

    def test_rejects_traversal_and_windows_aliases_before_writing(self):
        names = ["../escaped", "/absolute", "C:/drive", "safe\\escape", "a:stream", "a/./b", "a//b", "a./b", "NUL.txt", "a?/b"]
        for index, unsafe in enumerate(names):
            with self.subTest(member=unsafe):
                path, artifact = self.archive([("sdk/valid", b"valid"), (unsafe, b"bad")])
                output = self.root / f"out-{index}"
                with self.assertRaises(builder.BuildError):
                    builder.extract_zip(path, output, artifact, "sdk/")
                self.assertFalse(output.exists())
                path.unlink()

    def test_rejects_zip_links_and_reparse_flags(self):
        for index, attributes in enumerate(((stat.S_IFLNK | 0o777) << 16, builder.REPARSE_POINT)):
            info = zipfile.ZipInfo("sdk/link")
            info.create_system = 3
            info.external_attr = attributes
            path, artifact = self.archive([("sdk/valid", b"valid"), (info, b"../outside")])
            with self.assertRaises(builder.BuildError):
                builder.extract_zip(path, self.root / f"out-{index}", artifact, "sdk/")
            self.assertFalse((self.root / f"out-{index}").exists())
            path.unlink()

    def test_rejects_case_collisions_and_file_ancestors(self):
        for index, names in enumerate((("a.txt", "A.TXT"), ("a", "a/b"))):
            path, artifact = self.archive([(name, b"x") for name in names])
            with self.assertRaises(builder.BuildError):
                builder.extract_zip(path, self.root / f"out-{index}", artifact)
            self.assertFalse((self.root / f"out-{index}").exists())
            path.unlink()

    def test_rechecks_archive_bytes(self):
        path, artifact = self.archive([("file", b"x")])
        path.write_bytes(path.read_bytes() + b"modified")
        with self.assertRaises(builder.BuildError):
            builder.extract_zip(path, self.root / "out", artifact)
        self.assertFalse((self.root / "out").exists())

    def test_corrupt_cache_is_preserved_and_never_downloaded(self):
        cache = self.root / "cache"
        cache.mkdir()
        artifact = builder.ARTIFACTS[0]
        path = cache / artifact.filename
        path.write_bytes(b"not the pinned archive")
        with mock.patch.object(builder.urllib.request, "build_opener") as network:
            with self.assertRaises(builder.BuildError):
                builder.cached_artifact(artifact, cache)
            network.assert_not_called()
        self.assertEqual(path.read_bytes(), b"not the pinned archive")

    def test_valid_cache_reused_without_network(self):
        cache = self.root / "cache"
        cache.mkdir()
        path = cache / "trusted.zip"
        path.write_bytes(b"known bytes")
        artifact = replace(builder.ARTIFACTS[0], filename=path.name, digest=builder.hash_file(path))
        with mock.patch.object(builder.urllib.request, "build_opener") as network:
            self.assertEqual(builder.cached_artifact(artifact, cache), path)
            network.assert_not_called()

    def test_paths_require_fresh_external_output_and_receipt(self):
        repo = self.root / "repo"
        repo.mkdir()
        (repo / ".git").mkdir()
        output, cache, receipt = self.root / "out", self.root / "cache", self.root / "receipt.json"
        with self.assertRaises(builder.BuildError):
            builder.validate_paths(repo / "payload", cache, receipt, repo)
        output.mkdir()
        with self.assertRaises(builder.BuildError):
            builder.validate_paths(output, cache, receipt, repo)
        output.rmdir()
        receipt.write_text("immutable", encoding="utf-8")
        with self.assertRaises(builder.BuildError):
            builder.validate_paths(output, cache, receipt, repo)
        self.assertEqual(receipt.read_text(encoding="utf-8"), "immutable")

    def test_paths_cannot_overlap_cache(self):
        with self.assertRaises(builder.BuildError):
            builder.validate_paths(self.root / "cache/out", self.root / "cache", self.root / "receipt", self.root / "repo")

    def test_manifest_is_complete_sorted_and_excludes_itself(self):
        output = self.root / "out"
        output.mkdir()
        (output / "z").write_bytes(b"z")
        (output / "a").mkdir()
        (output / "a/first").write_bytes(b"a")
        (output / "manifest.json").write_bytes(b"old manifest")
        files = builder.payload_files(output)
        self.assertEqual([item["path"] for item in files], ["a/first", "z"])
        self.assertEqual(files[0]["sha256"], hashlib.sha256(b"a").hexdigest())

    def test_manifest_rejects_hardlink(self):
        output = self.root / "out"
        output.mkdir()
        (output / "a").write_bytes(b"a")
        os.link(output / "a", output / "b")
        with self.assertRaises(builder.BuildError):
            builder.payload_files(output)

    def lock(self):
        package = {"private": True, "dependencies": dict(builder.NPM_VERSIONS)}
        packages = {"": {"dependencies": dict(builder.NPM_VERSIONS)}}
        for name, version in builder.NPM_VERSIONS.items():
            packages["node_modules/" + name] = {"version": version, "resolved": f"https://registry.npmjs.org/{name}/-/{name}-{version}.tgz",
                "integrity": "sha512-" + base64.b64encode(b"x" * 64).decode("ascii")}
        return package, {"lockfileVersion": 3, "packages": packages}

    def test_lock_rejects_git_file_and_missing_integrity(self):
        package, lock = self.lock()
        builder.validate_lock(package, lock)
        item = lock["packages"]["node_modules/typescript"]
        for source in ("git+https://github.com/x/y.git", "file:../runtime", "https://example.com/a.tgz"):
            with self.subTest(source=source):
                item["resolved"] = source
                with self.assertRaises(builder.BuildError):
                    builder.validate_lock(package, lock)
        item["resolved"] = "https://registry.npmjs.org/typescript/-/typescript-6.0.3.tgz"
        item.pop("integrity")
        with self.assertRaises(builder.BuildError):
            builder.validate_lock(package, lock)

    def optional_lock(self, **metadata):
        entry = {"version": "2.3.3", "resolved": "https://registry.npmjs.org/fsevents/-/fsevents-2.3.3.tgz",
                 "integrity": "sha512-" + base64.b64encode(b"x" * 64).decode("ascii")}
        entry.update(metadata)
        return {"packages": {"": {}, "node_modules/fsevents": entry}}

    def test_platform_optional_absence_is_recorded(self):
        cases = ({"optional": True, "os": ["darwin"]}, {"optional": True, "cpu": ["arm64"]},
                 {"optional": True, "os": ["!win32"]}, {"optional": True, "cpu": "!x64"})
        for index, metadata in enumerate(cases):
            with self.subTest(metadata=metadata):
                output = self.root / f"optional-{index}"
                output.mkdir()
                builder.npm_inventory(output, self.optional_lock(**metadata))
                inventory = json.loads((output / "licenses/npm-packages.json").read_bytes())
                self.assertEqual(inventory[0]["status"], "NOT_INSTALLED_PLATFORM_OPTIONAL")
                self.assertEqual(inventory[0]["version"], "2.3.3")
                self.assertFalse((output / "node_modules/fsevents").exists())

    def test_missing_required_or_applicable_optional_package_fails(self):
        cases = ({"optional": False, "os": ["darwin"]}, {"os": ["darwin"]},
                 {"optional": True}, {"optional": True, "os": ["win32"], "cpu": ["x64"]},
                 {"optional": True, "os": ["any"]}, {"optional": True, "os": []},
                 {"optional": True, "os": ["!darwin"]}, {"optional": True, "os": [None]})
        for index, metadata in enumerate(cases):
            with self.subTest(metadata=metadata):
                output = self.root / f"required-{index}"
                output.mkdir()
                with self.assertRaises(builder.BuildError):
                    builder.npm_inventory(output, self.optional_lock(**metadata))
                self.assertFalse((output / "licenses/npm-packages.json").exists())

    def test_partial_optional_installation_is_not_silently_skipped(self):
        output = self.root / "partial"
        (output / "node_modules/fsevents").mkdir(parents=True)
        with self.assertRaises(builder.BuildError):
            builder.npm_inventory(output, self.optional_lock(optional=True, os=["darwin"]))

    def test_package_scripts_are_never_accepted(self):
        package, lock = self.lock()
        package["scripts"] = {"postinstall": "execute-repository-code"}
        with self.assertRaises(builder.BuildError):
            builder.validate_lock(package, lock)

    def test_dotnet_first_use_environment_disables_persistent_host_changes(self):
        work = self.root / "work"
        work.mkdir()
        keys = ("DOTNET_ADD_GLOBAL_TOOLS_TO_PATH", "DOTNET_GENERATE_ASPNET_CERTIFICATE", "DOTNET_CLI_HOME")
        # Host values must not opt child SDK processes back into first-use effects.
        with mock.patch.dict(os.environ, {keys[0]: "true", keys[1]: "true", keys[2]: "inherited-host-home"}):
            env = builder.build_environment(self.root / "payload", work)
            command = [sys.executable, "-c", "import json,os; print(json.dumps({k:os.environ[k] for k in " + repr(keys) + "}))"]
            result = subprocess.run(command, cwd=work, env=env, capture_output=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            observed = json.loads(result.stdout)
            self.assertEqual(observed, {keys[0]: "false", keys[1]: "false", keys[2]: str(work / "home")})
            self.assertEqual(os.environ[keys[0]], "true")
            self.assertEqual(os.environ[keys[1]], "true")
            self.assertEqual(os.environ[keys[2]], "inherited-host-home")

    def test_command_failure_records_actual_exit_and_logs(self):
        receipt = {"commands": []}
        runner = builder.Runner(self.root, receipt)
        with self.assertRaises(builder.BuildError):
            runner.run([sys.executable, "-c", "import sys; print('failed'); sys.exit(7)"], self.root, dict(os.environ), timeout=15)
        command = receipt["commands"][0]
        self.assertEqual(command["exit_code"], 7)
        self.assertIn(b"failed", Path(command["stdout"]).read_bytes())
        self.assertEqual(command["stdout_sha256"], builder.hash_file(Path(command["stdout"])))

    def test_receipt_is_create_only_and_json_is_parseable(self):
        path = self.root / "receipt.json"
        builder.write_new(path, builder.json_bytes({"status": "FAIL"}))
        with self.assertRaises(FileExistsError):
            builder.write_new(path, builder.json_bytes({"status": "PASS"}))
        self.assertEqual(json.loads(path.read_bytes()), {"status": "FAIL"})


if __name__ == "__main__":
    unittest.main()
