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
import struct
import subprocess
import sys
import tempfile
import io
import tarfile
import unittest
from unittest import mock
import zipfile

SOURCE = Path(__file__).resolve().parents[1] / "scripts/prepare_lsp_payload.py"
SPEC = importlib.util.spec_from_file_location("odin_payload_builder", SOURCE)
assert SPEC is not None and SPEC.loader is not None
builder = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = builder
SPEC.loader.exec_module(builder)
NOTICE_SPEC = importlib.util.spec_from_file_location("odin_native_notices", SOURCE.with_name("collect_lsp_notices.py"))
assert NOTICE_SPEC is not None and NOTICE_SPEC.loader is not None
notices = importlib.util.module_from_spec(NOTICE_SPEC)
NOTICE_SPEC.loader.exec_module(notices)


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

    def test_explicit_windows_separator_normalization_keeps_all_path_guards(self):
        path, artifact = self.archive([("licenses\\LICENSE", b"notice"), ("server.exe", b"binary")])
        builder.extract_zip(path, self.root / "winzip", artifact, windows_separators=True)
        self.assertEqual((self.root / "winzip/licenses/LICENSE").read_bytes(), b"notice")
        path.unlink()
        for index, names in enumerate((("..\\escape",), ("C:\\absolute",), ("\\absolute",),
                                       ("safe\\file", "safe/file"), ("safe\\NUL.txt",))):
            path, artifact = self.archive([(name, b"bad") for name in names])
            with self.assertRaises(builder.BuildError):
                builder.extract_zip(path, self.root / f"unsafe-{index}", artifact, windows_separators=True)
            self.assertFalse((self.root / f"unsafe-{index}").exists())
            path.unlink()

    def test_source_sidecar_is_deterministic_and_rechecks_each_archive(self):
        source = self.root / "source.tar.gz"
        source.write_bytes(b"inert compressed upstream source")
        item = {"id": "source", "member": "source.tar.gz", "sha256": builder.hash_file(source)}
        sidecar = {"destination": "sources/sidecar.zip", "readme": "NOT_RUN rebuild", "index": {"rebuild": "NOT_RUN"},
                   "inputs": [item], "sha256": "unknown"}
        # Compute the independently expected ZIP bytes using the public ZIP format.
        expected = self.root / "expected.zip"
        with zipfile.ZipFile(expected, "x") as archive:
            for name, data in (("README-SOURCE.txt", b"NOT_RUN rebuild"),
                               ("source-index.json", builder.json_bytes({"rebuild": "NOT_RUN"})),
                               ("source.tar.gz", source.read_bytes())):
                info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_STORED
                info.external_attr = 0o100644 << 16
                archive.writestr(info, data)
        sidecar["sha256"] = builder.hash_file(expected)
        builder.write_source_sidecar(self.root / "one", sidecar, {"source": source})
        builder.write_source_sidecar(self.root / "two", sidecar, {"source": source})
        self.assertEqual((self.root / "one/sources/sidecar.zip").read_bytes(), expected.read_bytes())
        self.assertEqual((self.root / "two/sources/sidecar.zip").read_bytes(), expected.read_bytes())
        source.write_bytes(b"tampered")
        with self.assertRaises(builder.BuildError):
            builder.write_source_sidecar(self.root / "tampered", sidecar, {"source": source})

    def native_crate(self, entries):
        path = self.root / "source.crate"
        with tarfile.open(path, "w:gz") as archive:
            for name, content in entries:
                member = tarfile.TarInfo("fixture-1/" + name)
                member.size = len(content)
                archive.addfile(member, io.BytesIO(content))
        return path

    def test_notice_collector_reads_archives_as_data_and_preserves_raw_notices(self):
        marker = self.root / "HOOK_EXECUTED"
        path = self.native_crate([("Cargo.toml", b'[package]\nname="fixture"\nversion="1"\nlicense="MIT"\nauthors=["Actual upstream author"]\n'),
                                  ("LICENSE", b"raw upstream notice\r\n"),
                                  ("build.rs", ('fn main() { panic!("' + str(marker) + '"); }').encode())])
        metadata = {"notice_sets": [{"id": "fixture", "scope": "CARGO_LOCK_SUPERSET_NOT_LINKED_BINARY_SBOM",
                    "applicability": "UNKNOWN", "sources": [{"id": "fixture", "package": "fixture", "version": "1",
                    "filename": path.name, "kind": "crate", "url": "https://static.crates.io/crates/fixture/fixture-1.crate",
                    "sha256": builder.hash_file(path)}]}]}
        result = notices.collect(metadata, self.root, self.root / "notices")
        self.assertEqual(result["groups"][0]["missing_notice_text"], 0)
        self.assertIn(b"raw upstream notice\r\n", (self.root / "notices/fixture/THIRD-PARTY-NOTICES.txt").read_bytes())
        inventory = json.loads((self.root / "notices/fixture/inventory.json").read_bytes())
        self.assertEqual(inventory["packages"][0]["authors"], ["Actual upstream author"])
        self.assertFalse(marker.exists())

    def test_notice_collector_records_missing_text_without_inventing_author(self):
        path = self.native_crate([("Cargo.toml", b'[package]\nname="fixture"\nversion="1"\nlicense="MIT"\n')])
        metadata = {"notice_sets": [{"id": "fixture", "scope": "LOCK_SUPERSET", "applicability": "UNKNOWN",
                    "sources": [{"id": "fixture", "package": "fixture", "version": "1", "filename": path.name,
                    "kind": "crate", "url": "https://static.crates.io/crates/fixture/fixture-1.crate", "sha256": builder.hash_file(path)}]}]}
        result = notices.collect(metadata, self.root, self.root / "missing")
        self.assertEqual(result["status"], "WITH_DECLARED_NOTICE_GAPS")
        missing = result["groups"][0]["unresolved"][0]
        self.assertEqual(missing["license"], "MIT")
        self.assertEqual(missing["authors"], [])
        self.assertEqual(missing["applicability"], "UNKNOWN")

    def test_notice_collector_rejects_changed_archive_and_existing_output(self):
        source = self.root / "source.crate"
        source.write_bytes(b"changed bytes")
        metadata = {"notice_sets": [{"id": "fixture", "scope": "LOCK_SUPERSET", "applicability": "UNKNOWN",
                    "sources": [{"id": "fixture", "package": "fixture", "version": "1", "filename": source.name,
                    "kind": "crate", "sha256": "0" * 64}]}]}
        with self.assertRaisesRegex(ValueError, "SHA256 mismatch"):
            notices.collect(metadata, self.root, self.root / "changed")
        output = self.root / "existing"
        output.mkdir()
        with self.assertRaisesRegex(ValueError, "fresh and absent"):
            notices.collect(metadata, self.root, output)

    def test_nuget_notice_collection_includes_runtime_third_party_text(self):
        data = io.BytesIO()
        with zipfile.ZipFile(data, "w") as archive:
            archive.writestr("runtime.nuspec", '<package><metadata><license type="expression">MIT</license><authors>Microsoft</authors></metadata></package>')
            archive.writestr("THIRD-PARTY-NOTICES.TXT", b"runtime third party notices")
        metadata, files = notices.archive_notices(data.getvalue(), "nuget")
        self.assertEqual(metadata["authors"], "Microsoft")
        self.assertIn(("THIRD-PARTY-NOTICES.TXT", b"runtime third party notices"), files)

    def pe_fixture(self, name="MSVCP140.dll", delay=False):
        data = bytearray(1024)
        data[:2] = b"MZ"
        struct.pack_into("<I", data, 60, 128)
        data[128:132] = b"PE\0\0"
        struct.pack_into("<HH", data, 132, 0x8664, 1)
        struct.pack_into("<H", data, 148, 240)
        struct.pack_into("<H", data, 152, 0x20B)
        struct.pack_into("<I", data, 272, 0x1000)
        struct.pack_into("<I", data, 368, 0x1050 if delay else 0)
        struct.pack_into("<III", data, 400, 512, 0x1000, 512)
        struct.pack_into("<I", data, 412, 512)
        struct.pack_into("<I", data, 524, 0x1040)
        encoded = name.encode("ascii") + b"\0"
        data[576:576 + len(encoded)] = encoded
        path = self.root / "fixture.exe"
        path.write_bytes(data)
        return path

    def test_pe_direct_imports_parse_and_unreviewed_delay_or_truncation_fail(self):
        self.assertEqual(builder.pe_imports(self.pe_fixture()), ["msvcp140.dll"])
        with self.assertRaisesRegex(builder.BuildError, "Delay-load"):
            builder.pe_imports(self.pe_fixture(delay=True))
        path = self.pe_fixture()
        path.write_bytes(path.read_bytes()[:540])
        with self.assertRaises(builder.BuildError):
            builder.pe_imports(path)

    def runtime_fixture(self):
        directory = self.root / "VC/Redist/MSVC/14.44.35112/x64/Microsoft.VC143.CRT"
        directory.mkdir(parents=True)
        dependencies = []
        for name in builder.MSVC_SHA256:
            source = directory / name
            source.write_bytes(name.encode())
            dependencies.append({"name": name, "source": str(source), "sha256": builder.hash_file(source)})
        graph = {"slint-lsp.exe": ["msvcp140.dll", "vcruntime140.dll", "user32.dll"],
                 "msvcp140.dll": ["vcruntime140.dll", "vcruntime140_1.dll"],
                 "vcruntime140.dll": ["kernel32.dll"], "vcruntime140_1.dll": ["kernel32.dll"]}
        return directory, dependencies, graph

    def test_recursive_runtime_requires_missing_dll_and_unknown_dependency_rejection(self):
        directory, dependencies, graph = self.runtime_fixture()
        pins = {item["name"]: item["sha256"] for item in dependencies}
        with mock.patch.object(builder, "MSVC_SHA256", pins), mock.patch.object(builder, "pe_imports", side_effect=lambda p: graph[p.name]):
            actual, system = builder.runtime_dependencies(self.root / "slint-lsp.exe", directory)
            self.assertEqual({item["name"] for item in actual}, set(pins))
            self.assertIn("user32.dll", system)
            (directory / "vcruntime140_1.dll").unlink()
            with self.assertRaisesRegex(builder.BuildError, "absent"):
                builder.runtime_dependencies(self.root / "slint-lsp.exe", directory)
            graph["slint-lsp.exe"] = ["unclassified-plugin.dll"]
            with self.assertRaisesRegex(builder.BuildError, "Unclassified"):
                builder.runtime_dependencies(self.root / "slint-lsp.exe", directory)
        with self.assertRaisesRegex(builder.BuildError, "official Visual Studio"):
            builder.runtime_dependencies(self.root / "slint-lsp.exe", self.root / "arbitrary")

    def test_runtime_wrong_signature_signer_and_version_refuse_publication(self):
        directory, dependencies, _ = self.runtime_fixture()
        rows = [{**item, "version": builder.MSVC_VERSION, "signature": "Valid", "signer": "CN=Microsoft Windows, O=Microsoft Corporation"} for item in dependencies]
        runner = builder.Runner(self.root, {"inputs": {}, "commands": []})
        for key, value in (("signature", "NotSigned"), ("signer", "O=Untrusted"), ("version", "0.0")):
            changed = [dict(row) for row in rows]
            changed[0][key] = value
            output = self.root / key
            with mock.patch.object(builder, "runtime_dependencies", return_value=(dependencies, [])), mock.patch.object(builder, "runtime_signatures", return_value=changed):
                with self.assertRaisesRegex(builder.BuildError, "signature/version/source"):
                    builder.add_slint_runtime(output, directory, runner, {})
            self.assertFalse((output / "slint").exists())

    def test_runtime_conflict_and_post_signature_source_change_are_preserved(self):
        directory, dependencies, _ = self.runtime_fixture()
        rows = [{**item, "version": builder.MSVC_VERSION, "signature": "Valid", "signer": "O=Microsoft Corporation"} for item in dependencies]
        runner = builder.Runner(self.root, {"inputs": {}, "commands": []})
        output = self.root / "conflict"
        (output / "slint").mkdir(parents=True)
        target = output / "slint" / rows[0]["name"]
        target.write_bytes(b"operator conflict")
        with mock.patch.object(builder, "runtime_dependencies", return_value=(dependencies, [])), mock.patch.object(builder, "runtime_signatures", return_value=rows):
            with self.assertRaisesRegex(builder.BuildError, "Refusing to replace"):
                builder.add_slint_runtime(output, directory, runner, {})
        self.assertEqual(target.read_bytes(), b"operator conflict")
        Path(rows[0]["source"]).write_bytes(b"changed after valid signature probe")
        with mock.patch.object(builder, "runtime_dependencies", return_value=(dependencies, [])), mock.patch.object(builder, "runtime_signatures", return_value=rows):
            with self.assertRaisesRegex(builder.BuildError, "changed after signature"):
                builder.add_slint_runtime(self.root / "changed", directory, runner, {})

    def test_runtime_signature_helper_is_fixed_and_does_not_change_host_policy(self):
        self.assertEqual(hashlib.sha256(builder.RUNTIME_SIGNATURE_SCRIPT.encode()).hexdigest(), builder.RUNTIME_SIGNATURE_SCRIPT_SHA256)
        self.assertNotIn("Set-ExecutionPolicy", builder.RUNTIME_SIGNATURE_SCRIPT)
        self.assertNotIn("Invoke-Expression", builder.RUNTIME_SIGNATURE_SCRIPT)

    def test_native_metadata_matches_reviewed_official_closure(self):
        data, components, artifacts = builder.native_inputs(builder.REPO / "packaging/lsp-native-sources.json")
        self.assertEqual(hashlib.sha256(data).hexdigest(), builder.NATIVE_SOURCES_SHA256)
        self.assertEqual({item["id"] for item in components},
                         {"slint-lsp", "clangd", "taplo", "marksman", "ruff", "ty", "rust-analyzer"})
        self.assertEqual(len({artifact.id for artifact in artifacts}), len(artifacts))
        for item in components:
            self.assertNotIn("latest", item["url"])
            self.assertTrue(item["entrypoint"].endswith(".exe"))
            self.assertEqual(len(item["sha256"]), 64)
            self.assertTrue(item["license_sources"] or item["id"] in {"slint-lsp", "clangd"})
        self.assertEqual(next(item for item in components if item["id"] == "clangd")["args"], ["--enable-config=false"])

    def test_native_source_metadata_tampering_rejected(self):
        path = self.root / "native.json"
        original = (builder.REPO / "packaging/lsp-native-sources.json").read_bytes()
        path.write_bytes(original + b" ")
        with self.assertRaises(builder.BuildError):
            builder.native_inputs(path)

    def test_native_zip_and_license_closure(self):
        path, artifact = self.archive([("sdk/server.exe", b"binary"), ("sdk/LICENSE", b"license")])
        notice = self.root / "NOTICE"
        notice.write_bytes(b"upstream notice")
        component = {"id": artifact.id, "version": artifact.version, "filename": artifact.filename,
                     "url": artifact.url, "sha256": artifact.digest, "archive": True, "directory": "server",
                     "prefix": "sdk/", "entrypoint": "server/server.exe", "license_sources": [
                         {"sha256": builder.hash_file(notice), "destination": "licenses/server/NOTICE"}]}
        output = self.root / "native"
        builder.extract_native({"test": path, "test-license-0": notice}, [component], output)
        self.assertEqual((output / "server/server.exe").read_bytes(), b"binary")
        self.assertEqual((output / "server/LICENSE").read_bytes(), b"license")
        self.assertEqual((output / "licenses/server/NOTICE").read_bytes(), b"upstream notice")

    def test_raw_binary_changed_after_validation_is_rejected(self):
        path = self.root / "server.exe"
        path.write_bytes(b"original")
        component = {"id": "server", "version": "1", "filename": path.name,
                     "url": "https://github.com/org/server/releases/download/v1/server.exe",
                     "sha256": builder.hash_file(path), "archive": False, "entrypoint": "server/server.exe",
                     "license_sources": []}
        path.write_bytes(b"changed")
        with self.assertRaises(builder.BuildError):
            builder.extract_native({"server": path}, [component], self.root / "native")
        self.assertFalse((self.root / "native").exists())

    def test_native_entrypoint_absence_fails(self):
        path, artifact = self.archive([("different.exe", b"binary")])
        component = {"id": artifact.id, "version": artifact.version, "filename": artifact.filename,
                     "url": artifact.url, "sha256": artifact.digest, "archive": True,
                     "directory": "server", "prefix": "", "entrypoint": "server/required.exe", "license_sources": []}
        with self.assertRaises(builder.BuildError):
            builder.extract_native({"test": path}, [component], self.root / "native")

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
