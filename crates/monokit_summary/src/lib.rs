//! `monokit_summary` — Multi-level codebase summaries optimised for LLM consumption.
//!
//! This crate analyses source files across many programming languages and produces
//! structured summaries at three levels of detail so that an LLM can quickly
//! understand a codebase's public surface without reading full implementations.
//!
//! # Summary levels
//!
//! - **[`Level::Index`]** — File paths with public symbol names only. The most
//!   compact representation; lets the LLM know *what exists* and *where*.
//!
//! - **[`Level::Signature`]** — For each public item: name, full type signature,
//!   first doc-comment line, file path, and line number. Enough for the LLM to
//!   understand the *shape* of the API without seeing bodies.
//!
//! - **[`Level::Outline`]** — Everything in `Signature`, plus full doc comments,
//!   trait bounds, and associated-item details. Still no implementation bodies,
//!   but the maximum context for deciding what to deep-dive into.
//!
//! # Supported languages
//!
//! | Language | Strategy | Signature detail |
//! |----------|----------|-----------------|
//! | Rust | AST (`syn`) | Full fn signatures, struct/enum/trait headers |
//! | TypeScript / JavaScript | Regex | Function/class/interface/type/enum/const |
//! | Go | Regex | Func/type/const/var (uppercase = exported) |
//! | Python | Regex | def / class with type hints |
//! | Others | Generic regex | Best-effort function detection |
//!
//! # Quick start
//!
//! ```no_run
//! use monokit_summary::{summarise, render_text, Level};
//!
//! let summary = summarise("/path/to/project", Level::Signature)?;
//! let text = render_text(&summary);
//! print!("{text}");
//! # Ok::<(), anyhow::Error>(())
//! ```

mod extract;
mod lang;
mod render;

pub mod types;

use std::path::Path;

use anyhow::Result;
pub use types::FileSummary;
pub use types::ItemKind;
pub use types::Language;
pub use types::Level;
pub use types::PublicItem;
pub use types::Summary;

/// Analyse every source file under `root` (skipping `target/`, `.git/`,
/// `node_modules/`) and return a [`Summary`] at the requested level.
///
/// This is the primary entry point. For single-file analysis see
/// [`summarise_file`].
pub fn summarise(root: &str, level: Level) -> Result<Summary> {
	let root = Path::new(root);
	let mut file_summaries = Vec::new();

	for file_path in collect_source_files(root)? {
		let source = std::fs::read_to_string(&file_path)?;
		let summary = summarise_file(&file_path, &source, level)?;
		if !summary.items.is_empty() {
			file_summaries.push(summary);
		}
	}

	file_summaries.sort_by(|a, b| a.file_path.cmp(&b.file_path));

	Ok(Summary {
		root: root.to_path_buf(),
		level,
		files: file_summaries,
	})
}

/// Analyse a single source file and return a summary at the requested level.
///
/// Language is detected from the file extension.
pub fn summarise_file(file_path: &Path, source: &str, level: Level) -> Result<FileSummary> {
	let items = extract::extract_public_items(file_path, source, level)?;
	Ok(FileSummary {
		file_path: file_path.to_path_buf(),
		level,
		items,
	})
}

/// Render a [`Summary`] as human-readable plain text (LLM-friendly).
pub fn render_text(summary: &Summary) -> String {
	render::render(summary)
}

/// Render a [`Summary`] as pretty-printed JSON.
pub fn render_json(summary: &Summary) -> Result<String> {
	render::render_json(summary)
}

/// Render a [`FileSummary`] as human-readable plain text (LLM-friendly).
pub fn render_file_text(file_summary: &FileSummary) -> String {
	let summary = Summary {
		root: file_summary.file_path.clone(),
		level: file_summary.level,
		files: vec![file_summary.clone()],
	};
	render::render(&summary)
}

/// Render a [`FileSummary`] as pretty-printed JSON.
pub fn render_file_json(file_summary: &FileSummary) -> Result<String> {
	let summary = Summary {
		root: file_summary.file_path.clone(),
		level: file_summary.level,
		files: vec![file_summary.clone()],
	};
	render::render_json(&summary)
}

/// Detect the [`Language`] for the given file path based on its extension.
pub fn language_from_path(path: &Path) -> Language {
	lang::language_from_path(path)
}

// ---------------------------------------------------------------------------
// File collection
// ---------------------------------------------------------------------------

/// Recursively collect source files under `root`, skipping common
/// non-source directories.
fn collect_source_files(root: &Path) -> Result<Vec<std::path::PathBuf>> {
	const SKIP: &[&str] = &[
		"target",
		".git",
		"node_modules",
		".direnv",
		".devenv",
		"__pycache__",
		"vendor",
		"build",
		"dist",
		".next",
		".nuxt",
	];

	// Extensions for languages we can summarise.
	const SOURCE_EXTS: &[&str] = &[
		"rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "go", "zig", "c", "h", "cpp", "cxx", "cc",
		"hpp", "hxx", "ixx", "java", "kt", "kts", "swift", "cs", "scala", "sc", "hs", "lhs", "elm",
		"ml", "mli", "py", "pyi", "pyw", "rb", "erb", "lua", "pl", "pm", "php", "r", "R", "sh",
		"bash", "zsh", "fish", "dart", "ex", "exs", "gleam", "nim", "nims", "m", "mm", "fs", "fsi",
		"fsx", "jl", "sol", "nix", "toml", "yaml", "yml", "json", "jsonc", "proto", "md", "mdx",
		"html", "htm", "css", "scss", "sass", "less", "svelte", "vue",
	];

	let mut result = Vec::new();
	collect_recursive(root, SKIP, SOURCE_EXTS, &mut result)?;
	result.sort();
	Ok(result)
}

fn collect_recursive(
	dir: &Path,
	skip: &[&str],
	exts: &[&str],
	out: &mut Vec<std::path::PathBuf>,
) -> Result<()> {
	for entry in std::fs::read_dir(dir)? {
		let entry = entry?;
		let name = entry.file_name();
		let name_str = name.to_string_lossy();

		if skip.contains(&name_str.as_ref()) || name_str.starts_with('.') {
			continue;
		}

		let path = entry.path();
		if path.is_dir() {
			collect_recursive(&path, skip, exts, out)?;
		} else if path
			.extension()
			.and_then(|e| e.to_str())
			.is_some_and(|e| exts.contains(&e))
		{
			out.push(path);
		}
	}
	Ok(())
}
