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
}

macro_rules! adapter {
    ($id:literal, [$($ext:literal),+], $server:literal, [$($arg:literal),*], [$($marker:literal),*], $url:literal) => {
        LanguageAdapter { id:$id, extensions:&[$($ext),+], server:$server,
            args:&[$($arg),*], project_markers:&[$($marker),*], upstream:$url }
    }
}

pub const ADAPTERS: &[LanguageAdapter] = &[
    adapter!(
        "rust",
        ["rs"],
        "rust-analyzer",
        [],
        ["Cargo.toml"],
        "https://rust-analyzer.github.io/"
    ),
    adapter!(
        "python",
        ["py", "pyi"],
        "basedpyright-langserver",
        ["--stdio"],
        ["pyproject.toml", "pyrightconfig.json"],
        "https://docs.basedpyright.com/"
    ),
    adapter!(
        "typescript",
        ["ts", "tsx", "mts", "cts"],
        "typescript-language-server",
        ["--stdio"],
        ["tsconfig.json"],
        "https://github.com/typescript-language-server/typescript-language-server"
    ),
    adapter!(
        "javascript",
        ["js", "jsx", "mjs", "cjs"],
        "typescript-language-server",
        ["--stdio"],
        ["jsconfig.json", "package.json"],
        "https://github.com/typescript-language-server/typescript-language-server"
    ),
    adapter!(
        "go",
        ["go"],
        "gopls",
        [],
        ["go.mod", "go.work"],
        "https://go.dev/gopls/"
    ),
    adapter!(
        "java",
        ["java"],
        "jdtls",
        [],
        ["pom.xml", "build.gradle", "build.gradle.kts"],
        "https://github.com/eclipse-jdtls/eclipse.jdt.ls"
    ),
    adapter!(
        "csharp",
        ["cs", "csx"],
        "csharp-ls",
        [],
        ["global.json", "Directory.Build.props"],
        "https://github.com/razzmatazz/csharp-language-server"
    ),
    adapter!(
        "cpp",
        ["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx"],
        "clangd",
        [],
        ["compile_commands.json", ".clangd", "CMakeLists.txt"],
        "https://clangd.llvm.org/"
    ),
    adapter!(
        "ruby",
        ["rb", "rake", "gemspec"],
        "ruby-lsp",
        [],
        ["Gemfile"],
        "https://shopify.github.io/ruby-lsp/"
    ),
    adapter!(
        "php",
        ["php"],
        "phpactor",
        ["language-server"],
        ["composer.json"],
        "https://phpactor.readthedocs.io/"
    ),
    adapter!(
        "lua",
        ["lua"],
        "lua-language-server",
        [],
        [".luarc.json"],
        "https://luals.github.io/"
    ),
    adapter!(
        "swift",
        ["swift"],
        "sourcekit-lsp",
        [],
        ["Package.swift"],
        "https://github.com/swiftlang/sourcekit-lsp"
    ),
    adapter!(
        "kotlin",
        ["kt", "kts"],
        "kotlin-lsp",
        [],
        ["settings.gradle.kts"],
        "https://github.com/Kotlin/kotlin-lsp"
    ),
    adapter!(
        "scala",
        ["scala", "sc"],
        "metals",
        [],
        ["build.sbt"],
        "https://scalameta.org/metals/"
    ),
    adapter!(
        "dart",
        ["dart"],
        "dart",
        ["language-server", "--protocol=lsp"],
        ["pubspec.yaml"],
        "https://dart.dev/tools/dart-language-server"
    ),
    adapter!(
        "elixir",
        ["ex", "exs"],
        "elixir-ls",
        [],
        ["mix.exs"],
        "https://github.com/elixir-lsp/elixir-ls"
    ),
    adapter!(
        "haskell",
        ["hs", "lhs"],
        "haskell-language-server-wrapper",
        ["--lsp"],
        ["cabal.project", "stack.yaml"],
        "https://haskell-language-server.readthedocs.io/"
    ),
    adapter!(
        "ocaml",
        ["ml", "mli"],
        "ocamllsp",
        [],
        ["dune-project"],
        "https://github.com/ocaml/ocaml-lsp"
    ),
    adapter!(
        "r",
        ["r", "R"],
        "R",
        ["--slave", "-e", "languageserver::run()"],
        ["DESCRIPTION"],
        "https://github.com/REditorSupport/languageserver"
    ),
    adapter!(
        "zig",
        ["zig", "zon"],
        "zls",
        [],
        ["build.zig"],
        "https://zigtools.org/zls/"
    ),
    adapter!(
        "bash",
        ["sh", "bash"],
        "bash-language-server",
        ["start"],
        [".shellcheckrc"],
        "https://github.com/bash-lsp/bash-language-server"
    ),
    adapter!(
        "html",
        ["html", "htm"],
        "vscode-html-language-server",
        ["--stdio"],
        [],
        "https://github.com/microsoft/vscode-html-languageservice"
    ),
    adapter!(
        "css",
        ["css", "scss", "less"],
        "vscode-css-language-server",
        ["--stdio"],
        [],
        "https://github.com/microsoft/vscode-css-languageservice"
    ),
    adapter!(
        "json",
        ["json", "jsonc"],
        "vscode-json-language-server",
        ["--stdio"],
        [],
        "https://github.com/microsoft/vscode-json-languageservice"
    ),
    adapter!(
        "yaml",
        ["yaml", "yml"],
        "yaml-language-server",
        ["--stdio"],
        [],
        "https://github.com/redhat-developer/yaml-language-server"
    ),
    adapter!(
        "vue",
        ["vue"],
        "vue-language-server",
        ["--stdio"],
        ["package.json"],
        "https://github.com/vuejs/language-tools"
    ),
    adapter!(
        "svelte",
        ["svelte"],
        "svelteserver",
        ["--stdio"],
        ["svelte.config.js"],
        "https://github.com/sveltejs/language-tools"
    ),
    adapter!(
        "terraform",
        ["tf", "tfvars"],
        "terraform-ls",
        ["serve"],
        [".terraform.lock.hcl"],
        "https://github.com/hashicorp/terraform-ls"
    ),
];

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
    fn registry_ids_are_unique_with_nonempty_document_extensions() {
        let mut ids = std::collections::HashSet::new();
        for adapter in ADAPTERS {
            assert!(ids.insert(adapter.id));
            assert!(!adapter.extensions.is_empty());
            assert!(adapter.upstream.starts_with("https://"));
        }
    }
}
