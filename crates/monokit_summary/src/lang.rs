//! Language detection from file extensions.
//!
//! Maps file extensions to [`Language`] variants. The mapping is aligned
//! with the LSP language definitions in `monokit_config` and Zed's
//! tree-sitter grammars.

use crate::types::Language;

/// Map a file extension (without the dot) to a [`Language`].
pub(crate) fn language_from_ext(ext: &str) -> Language {
	match ext.to_ascii_lowercase().as_str() {
		// Systems / statically-typed
		"rs" => Language::Rust,
		"ts" | "tsx" => Language::TypeScript,
		"js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
		"go" => Language::Go,
		"zig" | "zon" => Language::Zig,
		"c" | "h" => Language::C,
		"cpp" | "cxx" | "cc" | "hpp" | "hxx" | "ixx" => Language::Cpp,
		"java" => Language::Java,
		"kt" | "kts" => Language::Kotlin,
		"swift" => Language::Swift,
		"cs" | "fs" => Language::CSharp,
		"scala" | "sc" => Language::Scala,
		"hs" | "lhs" => Language::Haskell,
		"elm" => Language::Elm,
		"ml" | "mli" => Language::OCaml,
		"dart" => Language::Dart,
		"ex" | "exs" => Language::Elixir,
		"gleam" => Language::Gleam,
		"nim" | "nims" => Language::Nim,
		"m" | "mm" => Language::ObjectiveC,
		"fsx" | "fsi" => Language::FSharp,
		"jl" => Language::Julia,
		"r" | "R" => Language::R,
		// Dynamically-typed / scripting
		"py" | "pyi" | "pyw" => Language::Python,
		"rb" | "erb" => Language::Ruby,
		"lua" => Language::Lua,
		"pl" | "pm" | "t" => Language::Perl,
		"php" | "phtml" => Language::Php,
		"sh" | "bash" | "zsh" | "fish" => Language::Shell,
		// Web / templating
		"html" | "htm" => Language::Html,
		"css" | "scss" | "sass" | "less" => Language::Css,
		"svelte" => Language::Svelte,
		"vue" => Language::Vue,
		// Markup / config / data
		"toml" => Language::Toml,
		"yaml" | "yml" => Language::Yaml,
		"json" | "jsonc" => Language::Json,
		"proto" | "protobuf" => Language::Protobuf,
		// Docs
		"md" | "mdx" => Language::Markdown,
		// Smart-contract / niche
		"sol" => Language::Solidity,
		"nix" => Language::Nix,
		_ => Language::Unknown,
	}
}

/// Detect the language of a file from its path based on its extension.
pub(crate) fn language_from_path(path: &std::path::Path) -> Language {
	path.extension()
		.and_then(|e| e.to_str())
		.map_or(Language::Unknown, language_from_ext)
}
