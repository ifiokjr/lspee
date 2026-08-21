//! Core types for codebase summaries.
//!
//! The summary model is deliberately language-agnostic. Every language
//! extractor produces the same [`PublicItem`] shape so that consumers
//! (LLMs, MCP tools, search indexes) can treat the output uniformly.
//!
//! # Design notes
//!
//! The [`Language`] enum and [`ItemKind`] enum are aligned with Zed's
//! outline capture model (`@item`, `@name`, `@context`) and the LSP
//! `DocumentSymbol` shape, making it straightforward to add tree-sitter
//! or LSP-backed extractors later.

use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Summary levels
// ---------------------------------------------------------------------------

/// Detail level for the generated summary.
///
/// Each level is a strict superset of the one below it, so requesting
/// [`Level::Outline`] includes everything [`Level::Signature`] includes
/// and so on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
	/// Most compact — just file paths and public symbol names.
	/// Enough for the LLM to know *what exists* and *where*.
	Index,
	/// Medium — symbol name, full type signature, first doc line,
	/// file path, and line number. Enough for the LLM to understand
	/// the *shape* of the API without seeing bodies.
	Signature,
	/// Most detailed — everything in [`Signature`](Level::Signature) plus
	/// full doc comments, trait bounds, and associated-item details.
	/// Still no implementation bodies, but maximum context for deciding
	/// where to deep-dive.
	Outline,
}

// ---------------------------------------------------------------------------
// Item kinds
// ---------------------------------------------------------------------------

/// The kind of a public item.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
	/// `fn` — a free-standing or associated function.
	Function,
	/// `struct` — a struct definition.
	Struct,
	/// `enum` — an enum definition.
	Enum,
	/// `trait` — a trait definition.
	Trait,
	/// `trait alias` — a trait alias.
	TraitAlias,
	/// `type` — a type alias.
	TypeAlias,
	/// `const` — a constant item.
	Const,
	/// `static` — a static item.
	Static,
	/// `mod` — a module declaration.
	Module,
	/// `impl` — an impl block (inherent or trait).
	Impl,
	/// `macro` — a `macro_rules!` or declarative macro.
	Macro,
	/// `interface` — TypeScript/Go interface.
	Interface,
	/// `class` — Python/TypeScript/Java class.
	Class,
	/// `namespace` — C++/TS namespace.
	Namespace,
	/// `var` — top-level exported variable (JS/TS/Go).
	Var,
	/// Any item that doesn't fit the above.
	Other(String),
}

// ---------------------------------------------------------------------------
// Public item
// ---------------------------------------------------------------------------

/// A single public item extracted from a source file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PublicItem {
	/// Human-readable name of the item (e.g. `"HashMap"`, `"parse"`).
	pub name: String,
	/// What kind of item this is.
	pub kind: ItemKind,
	/// Line number where the item is defined (1-based).
	pub line: u32,
	/// Full type signature, if the language provides one.
	///
	/// Examples:
	/// - Rust:    `fn parse(input: &str) -> Result<Ast>`
	/// - TypeScript: `function parse(input: string): Ast`
	/// - Python:  `def parse(input: str) -> Ast`
	/// - Go:      `func Parse(input string) (Ast, error)`
	///
	/// `None` when the language is too dynamic to infer a type
	/// (e.g. plain JavaScript without `JSDoc`).
	pub signature: Option<String>,
	/// The first line of the item's doc comment, if any.
	///
	/// This is always populated when docs exist, regardless of level.
	/// At [`Level::Outline`] the full doc comment is stored in
	/// [`full_docs`](PublicItem::full_docs) instead.
	pub first_doc_line: Option<String>,
	/// Full doc comment text (only present at [`Level::Outline`]).
	pub full_docs: Option<String>,
	/// Parent path for nested items (e.g. `["MyStruct", "impl"]` for
	/// an associated function, or `["my_mod"]` for a re-export).
	pub parent_path: Vec<String>,
}

// ---------------------------------------------------------------------------
// File / suite summaries
// ---------------------------------------------------------------------------

/// Summary of a single source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSummary {
	/// Path to the file, relative to the summarised root.
	pub file_path: PathBuf,
	/// The level at which this summary was generated.
	pub level: Level,
	/// Public items discovered in this file.
	pub items: Vec<PublicItem>,
}

/// Summary of an entire workspace / directory tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
	/// Root directory that was summarised.
	pub root: PathBuf,
	/// The level at which this summary was generated.
	pub level: Level,
	/// Per-file summaries, sorted by path.
	pub files: Vec<FileSummary>,
}

/// Language detected for a source file.
///
/// Covers the top 50+ programming languages by popularity, aligned with
/// the LSP definitions in `monokit_config` and the outline queries in
/// Zed's tree-sitter grammars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
	// ── Systems / statically-typed with rich signatures ──────────
	Rust,
	TypeScript,
	JavaScript,
	Go,
	Zig,
	C,
	Cpp,
	Java,
	Kotlin,
	Swift,
	CSharp,
	Scala,
	Haskell,
	Elm,
	OCaml,
	Dart,
	Elixir,
	Gleam,
	Nim,
	ObjectiveC,
	FSharp,
	Julia,
	R,
	// ── Dynamically-typed / scripting ────────────────────────────
	Python,
	Ruby,
	Lua,
	Perl,
	Php,
	Shell,
	// ── Web / templating ────────────────────────────────────────
	Html,
	Css,
	Svelte,
	Vue,
	// ── Markup / config / data ──────────────────────────────────
	Toml,
	Yaml,
	Json,
	Protobuf,
	// ── Docs ────────────────────────────────────────────────────
	Markdown,
	// ── Smart-contract / niche ──────────────────────────────────
	Solidity,
	Nix,
	// ── Catch-all ──────────────────────────────────────────────
	Unknown,
}
