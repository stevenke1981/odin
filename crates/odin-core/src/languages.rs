//! Declarative LSP profiles. Registry membership is configuration guidance, not
//! evidence that an executable is installed or that a backend passed acceptance.
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct LanguageAdapter {
    pub id: &'static str,
    pub extensions: &'static [&'static str],
    pub server: &'static str,
    pub args: &'static [&'static str],
    pub project_markers: &'static [&'static str],
    pub upstream: &'static str,
    /// Suggested install commands (alternatives, any one suffices). Guidance only:
    /// Odin never runs them. Empty means "follow `upstream`".
    pub install: &'static [&'static str],
}

macro_rules! adapter {
    ($id:literal, [$($ext:literal),+], $server:literal, [$($arg:literal),*], [$($marker:literal),*], $url:literal, [$($install:literal),*]) => {
        LanguageAdapter { id:$id, extensions:&[$($ext),+], server:$server,
            args:&[$($arg),*], project_markers:&[$($marker),*], upstream:$url, install:&[$($install),*] }
    }
}

pub const ADAPTERS: &[LanguageAdapter] = &[
    adapter!(
        "rust",
        ["rs"],
        "rust-analyzer",
        [],
        ["Cargo.toml"],
        "https://rust-analyzer.github.io/",
        ["rustup component add rust-analyzer"]
    ),
    adapter!(
        "python",
        ["py", "pyi"],
        "basedpyright-langserver",
        ["--stdio"],
        ["pyproject.toml", "pyrightconfig.json"],
        "https://docs.basedpyright.com/",
        ["pip install basedpyright", "uv tool install basedpyright"]
    ),
    adapter!(
        "typescript",
        ["ts", "tsx", "mts", "cts"],
        "typescript-language-server",
        ["--stdio"],
        ["tsconfig.json"],
        "https://github.com/typescript-language-server/typescript-language-server",
        ["npm install -g typescript typescript-language-server"]
    ),
    adapter!(
        "javascript",
        ["js", "jsx", "mjs", "cjs"],
        "typescript-language-server",
        ["--stdio"],
        ["jsconfig.json", "package.json"],
        "https://github.com/typescript-language-server/typescript-language-server",
        ["npm install -g typescript typescript-language-server"]
    ),
    adapter!(
        "go",
        ["go"],
        "gopls",
        [],
        ["go.mod", "go.work"],
        "https://go.dev/gopls/",
        ["go install golang.org/x/tools/gopls@latest"]
    ),
    adapter!(
        "java",
        ["java"],
        "jdtls",
        [],
        ["pom.xml", "build.gradle", "build.gradle.kts"],
        "https://github.com/eclipse-jdtls/eclipse.jdt.ls",
        ["brew install jdtls"]
    ),
    adapter!(
        "csharp",
        ["cs", "csx"],
        "csharp-ls",
        [],
        ["global.json", "Directory.Build.props"],
        "https://github.com/razzmatazz/csharp-language-server",
        ["dotnet tool install --global csharp-ls"]
    ),
    adapter!(
        "c",
        ["c", "h"],
        "clangd",
        ["--enable-config=false"],
        ["compile_commands.json", "CMakeLists.txt"],
        "https://clangd.llvm.org/",
        [
            "winget install LLVM.LLVM",
            "brew install llvm",
            "sudo apt install clangd"
        ]
    ),
    adapter!(
        "cpp",
        ["cc", "cpp", "cxx", "hpp", "hh", "hxx"],
        "clangd",
        ["--enable-config=false"],
        ["compile_commands.json", ".clangd", "CMakeLists.txt"],
        "https://clangd.llvm.org/",
        [
            "winget install LLVM.LLVM",
            "brew install llvm",
            "sudo apt install clangd"
        ]
    ),
    adapter!(
        "ruby",
        ["rb", "rake", "gemspec"],
        "ruby-lsp",
        [],
        ["Gemfile"],
        "https://shopify.github.io/ruby-lsp/",
        ["gem install ruby-lsp"]
    ),
    adapter!(
        "php",
        ["php"],
        "phpactor",
        ["language-server"],
        ["composer.json"],
        "https://phpactor.readthedocs.io/",
        []
    ),
    adapter!(
        "lua",
        ["lua"],
        "lua-language-server",
        [],
        [".luarc.json"],
        "https://luals.github.io/",
        ["brew install lua-language-server"]
    ),
    adapter!(
        "swift",
        ["swift"],
        "sourcekit-lsp",
        [],
        ["Package.swift"],
        "https://github.com/swiftlang/sourcekit-lsp",
        []
    ),
    adapter!(
        "kotlin",
        ["kt", "kts"],
        "kotlin-lsp",
        [],
        ["settings.gradle.kts"],
        "https://github.com/Kotlin/kotlin-lsp",
        ["brew install JetBrains/utils/kotlin-lsp"]
    ),
    adapter!(
        "scala",
        ["scala", "sc"],
        "metals",
        [],
        ["build.sbt"],
        "https://scalameta.org/metals/",
        ["cs install metals"]
    ),
    adapter!(
        "dart",
        ["dart"],
        "dart",
        ["language-server", "--protocol=lsp"],
        ["pubspec.yaml"],
        "https://dart.dev/tools/dart-language-server",
        []
    ),
    adapter!(
        "elixir",
        ["ex", "exs"],
        "elixir-ls",
        [],
        ["mix.exs"],
        "https://github.com/elixir-lsp/elixir-ls",
        ["brew install elixir-ls"]
    ),
    adapter!(
        "haskell",
        ["hs", "lhs"],
        "haskell-language-server-wrapper",
        ["--lsp"],
        ["cabal.project", "stack.yaml"],
        "https://haskell-language-server.readthedocs.io/",
        ["ghcup install hls"]
    ),
    adapter!(
        "ocaml",
        ["ml", "mli"],
        "ocamllsp",
        [],
        ["dune-project"],
        "https://github.com/ocaml/ocaml-lsp",
        ["opam install ocaml-lsp-server"]
    ),
    adapter!(
        "r",
        ["r", "R"],
        "R",
        ["--slave", "-e", "languageserver::run()"],
        ["DESCRIPTION"],
        "https://github.com/REditorSupport/languageserver",
        ["R -e \"install.packages('languageserver')\""]
    ),
    adapter!(
        "zig",
        ["zig", "zon"],
        "zls",
        [],
        ["build.zig"],
        "https://zigtools.org/zls/",
        ["brew install zls"]
    ),
    adapter!(
        "bash",
        ["sh", "bash"],
        "bash-language-server",
        ["start"],
        [".shellcheckrc"],
        "https://github.com/bash-lsp/bash-language-server",
        ["npm install -g bash-language-server"]
    ),
    adapter!(
        "html",
        ["html", "htm"],
        "vscode-html-language-server",
        ["--stdio"],
        [],
        "https://github.com/microsoft/vscode-html-languageservice",
        ["npm install -g vscode-langservers-extracted"]
    ),
    adapter!(
        "css",
        ["css", "scss", "less"],
        "vscode-css-language-server",
        ["--stdio"],
        [],
        "https://github.com/microsoft/vscode-css-languageservice",
        ["npm install -g vscode-langservers-extracted"]
    ),
    adapter!(
        "json",
        ["json", "jsonc"],
        "vscode-json-language-server",
        ["--stdio"],
        [],
        "https://github.com/microsoft/vscode-json-languageservice",
        ["npm install -g vscode-langservers-extracted"]
    ),
    adapter!(
        "yaml",
        ["yaml", "yml"],
        "yaml-language-server",
        ["--stdio"],
        [],
        "https://github.com/redhat-developer/yaml-language-server",
        ["npm install -g yaml-language-server"]
    ),
    adapter!(
        "slint",
        ["slint"],
        "slint-lsp",
        [],
        // Slint has no project file of its own; Cargo.toml would flag every Rust crate.
        [],
        "https://docs.slint.dev/latest/docs/language/",
        ["cargo install slint-lsp"]
    ),
    adapter!(
        "toml",
        ["toml"],
        "taplo",
        ["lsp", "stdio"],
        [],
        "https://taplo.tamasfe.dev/",
        ["cargo install taplo-cli --locked --features lsp"]
    ),
    adapter!(
        "markdown",
        ["md", "markdown"],
        "marksman",
        ["server"],
        [".marksman.toml"],
        "https://github.com/artempyanykh/marksman",
        ["brew install marksman"]
    ),
    adapter!(
        "vue",
        ["vue"],
        "vue-language-server",
        ["--stdio"],
        ["package.json"],
        "https://github.com/vuejs/language-tools",
        ["npm install -g @vue/language-server"]
    ),
    adapter!(
        "svelte",
        ["svelte"],
        "svelteserver",
        ["--stdio"],
        ["svelte.config.js"],
        "https://github.com/sveltejs/language-tools",
        ["npm install -g svelte-language-server"]
    ),
    adapter!(
        "terraform",
        ["tf", "tfvars"],
        "terraform-ls",
        ["serve"],
        [".terraform.lock.hcl"],
        "https://github.com/hashicorp/terraform-ls",
        ["brew install hashicorp/tap/terraform-ls"]
    ),
    adapter!(
        "gleam",
        ["gleam"],
        "gleam",
        ["lsp"],
        ["gleam.toml"],
        "https://gleam.run/install/",
        ["winget install --id Gleam.Gleam", "brew install gleam"]
    ),
    adapter!(
        "typst",
        ["typ"],
        "tinymist",
        [],
        ["typst.toml"],
        "https://github.com/Myriad-Dreamin/tinymist/releases",
        ["brew install tinymist"]
    ),
];

/// The registry profile for a host language entry: by ID first, then by server program
/// (host IDs may be custom). Returns None for programs Odin has no guidance for.
pub fn adapter_for(id: &str, program: &str) -> Option<&'static LanguageAdapter> {
    let program = Path::new(program)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(program);
    ADAPTERS
        .iter()
        .find(|adapter| adapter.id == id)
        .or_else(|| ADAPTERS.iter().find(|adapter| adapter.server == program))
}

fn install_note(id: &str) -> Option<&'static str> {
    match id {
        "java" => Some("Or download a jdtls milestone from the upstream page"),
        "php" => Some("Install the phpactor PHAR per the upstream guide"),
        "swift" => Some("Bundled with the Swift toolchain (swift.org) and Xcode"),
        "dart" => Some("Bundled with the Dart/Flutter SDK"),
        "lua" => Some("Or download an upstream release"),
        "zig" => Some("Or download an upstream release matching your Zig version"),
        "markdown" => Some("Or download an upstream release"),
        "elixir" => Some("Or download an upstream release"),
        "typst" => Some("Or download and extract the upstream Tinymist binary for your platform"),
        _ => None,
    }
}

/// Install suggestion for a missing or failing language server. Advisory only:
/// Odin never installs backends, so the host operator decides what to run.
pub fn install_guidance(adapter: &LanguageAdapter) -> serde_json::Value {
    serde_json::json!({
        "server": adapter.server,
        "commands": adapter.install,
        "note": install_note(adapter.id),
        "upstream": adapter.upstream,
        "automatic_installation": false,
        "after_install": "Restart the MCP server (or restart_language_server), then run odin doctor --probe-lsp",
    })
}

/// Multiplexed servers require the document's LSP ID, not the host profile name.
/// Custom host profile IDs are preserved; no source-text heuristics are used.
pub fn language_id_for_path<'a>(profile: &'a str, path: &Path) -> &'a str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match profile {
        "typescript" | "javascript" | "typescriptreact" | "javascriptreact" => match ext.as_str() {
            "tsx" => "typescriptreact",
            "jsx" => "javascriptreact",
            "ts" | "mts" | "cts" => "typescript",
            "js" | "mjs" | "cjs" => "javascript",
            _ => profile,
        },
        "cpp" | "c" => match ext.as_str() {
            "c" => "c",
            "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
            _ => profile,
        },
        "css" | "scss" | "less" => match ext.as_str() {
            "scss" => "scss",
            "less" => "less",
            "css" => "css",
            _ => profile,
        },
        "json" | "jsonc" if ext == "jsonc" => "jsonc",
        "bash" | "shellscript" => "shellscript",
        _ => profile,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplexed_ids_and_custom_profiles_are_not_conflated() {
        for (profile, file, expected) in [
            ("typescript", "ui.tsx", "typescriptreact"),
            ("typescript", "ui.jsx", "javascriptreact"),
            ("typescript", "module.mjs", "javascript"),
            ("javascript", "types.cts", "typescript"),
            ("cpp", "code.c", "c"),
            ("c", "code.cpp", "cpp"),
            ("css", "theme.scss", "scss"),
            ("json", "settings.jsonc", "jsonc"),
            ("typescript", "ui.TSX", "typescriptreact"),
            ("javascript", "ui.JSX", "javascriptreact"),
            ("bash", "sample.sh", "shellscript"),
            ("shellscript", "sample.bash", "shellscript"),
            ("custom-bash", "sample.sh", "custom-bash"),
            ("custom", "ui.tsx", "custom"),
        ] {
            assert_eq!(language_id_for_path(profile, Path::new(file)), expected);
        }
    }

    #[test]
    fn expanded_profiles_are_guidance_with_separate_c_and_cpp_extensions() {
        for (id, server, args) in [
            ("slint", "slint-lsp", &[][..]),
            ("c", "clangd", &["--enable-config=false"][..]),
            ("cpp", "clangd", &["--enable-config=false"][..]),
            ("toml", "taplo", &["lsp", "stdio"][..]),
            ("yaml", "yaml-language-server", &["--stdio"][..]),
            ("markdown", "marksman", &["server"][..]),
        ] {
            let profile = ADAPTERS.iter().find(|entry| entry.id == id).unwrap();
            assert_eq!(profile.server, server);
            assert_eq!(profile.args, args);
        }
        let c = ADAPTERS.iter().find(|entry| entry.id == "c").unwrap();
        let cpp = ADAPTERS.iter().find(|entry| entry.id == "cpp").unwrap();
        assert!(c.extensions.contains(&"c"));
        assert!(
            c.extensions
                .iter()
                .all(|extension| !cpp.extensions.contains(extension))
        );
    }

    #[test]
    fn every_profile_suggests_an_install_command_or_an_upstream_note() {
        for adapter in ADAPTERS {
            assert!(
                !adapter.install.is_empty() || install_note(adapter.id).is_some(),
                "{}",
                adapter.id
            );
            assert!(adapter.upstream.starts_with("https://"), "{}", adapter.id);
        }
        let rust = adapter_for("rust", "rust-analyzer").unwrap();
        assert_eq!(rust.install, ["rustup component add rust-analyzer"]);
        let guidance = install_guidance(rust);
        assert_eq!(guidance["automatic_installation"], false);
        assert_eq!(
            guidance["commands"][0],
            "rustup component add rust-analyzer"
        );
        assert_eq!(
            adapter_for("custom-rs", r"C:\Tools\rust-analyzer.exe").map(|a| a.id),
            Some("rust")
        );
        assert!(adapter_for("custom", "unknown-ls").is_none());
    }

    #[test]
    fn slint_is_detected_by_slint_sources_not_by_cargo_manifests() {
        let slint = ADAPTERS.iter().find(|entry| entry.id == "slint").unwrap();
        assert!(slint.project_markers.is_empty());
        let cargo_owners = ADAPTERS
            .iter()
            .filter(|entry| entry.project_markers.contains(&"Cargo.toml"))
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        assert_eq!(cargo_owners, ["rust"]);
    }

    #[test]
    fn gleam_and_typst_profiles_supply_detection_and_launch_metadata() {
        for (id, server, source, marker, args) in [
            ("gleam", "gleam", "main.gleam", "gleam.toml", &["lsp"][..]),
            ("typst", "tinymist", "main.typ", "typst.toml", &[][..]),
        ] {
            let profile = adapter_for(id, server).unwrap();
            let path = Path::new(source);
            let extension = path.extension().unwrap().to_str().unwrap();
            let owners = ADAPTERS
                .iter()
                .filter(|entry| entry.extensions.contains(&extension))
                .map(|entry| entry.id)
                .collect::<Vec<_>>();
            assert_eq!(owners, [id]);
            assert_eq!(profile.project_markers, [marker]);
            assert_eq!(profile.server, server);
            assert_eq!(profile.args, args);
            assert_eq!(language_id_for_path(profile.id, path), id);
            assert_eq!(install_guidance(profile)["automatic_installation"], false);
        }
    }

    #[test]
    fn registry_ids_are_unique_with_nonempty_document_extensions() {
        let mut ids = std::collections::HashSet::new();
        for adapter in ADAPTERS {
            assert!(ids.insert(adapter.id));
            assert!(!adapter.extensions.is_empty());
            assert!(adapter.upstream.starts_with("https://"));
        }
    }
}
