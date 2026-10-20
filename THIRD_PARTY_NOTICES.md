# Third-party notices

Odin's newly authored source is offered under the included MIT license. The design references public Serena functionality and standard protocol/SDK interfaces. This is not a formal clean-room claim or a legal compliance certification.

Odin 0.2.0 does not vendor Serena or SolidLSP source, does not depend on their packages, and removes the external Serena MCP bridge. Public references and the comparison matrix remain documentation only. No Serena or JetBrains plugin is included or required by Odin's native runtime.

Rust dependencies and external language servers/debug adapters retain their respective licenses. The real Cargo.lock is committed. `verification/sbom.cdx.json`, `verification/license-inventory.json`, and `docs/THIRD_PARTY_LICENSES.txt` inventory all resolved target variants, checksums and license texts. `cargo deny check licenses` and strict Cargo audit are executed separately. Some crates omit license files from their tarballs; exact upstream texts and their source hashes are recorded under `docs/dependency-licenses/`. External language/debug servers used for validation are not bundled. This is not a certified distribution of any third-party backend.

The Python test fixtures are newly authored protocol mocks, not actual language servers or debuggers. Python is test/verification tooling, not a dependency of the Odin Rust runtime; a separately selected backend may have its own runtime requirements.
