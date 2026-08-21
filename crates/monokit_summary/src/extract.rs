//! Public-item extraction per language.
//!
//! Each extractor takes a source string and a detail level, and returns a
//! list of [`PublicItem`](crate::types::PublicItem)s. The Rust extractor
//! uses `syn` for precise AST parsing; tree-sitter is used for other
//! supported languages when the `outline` feature is enabled; regex-based
//! heuristics serve as the fallback.

use crate::lang::language_from_path;
use crate::types::Language;
use crate::types::Level;
use crate::types::PublicItem;

#[cfg(feature = "outline")]
mod ts_query;

mod rust;
mod text;

/// Extract public items from a single source file, dispatching to the
/// correct extractor based on the detected language.
///
/// Priority: `syn` (Rust only) > tree-sitter (when grammar available) >
/// regex heuristics.
pub(crate) fn extract_public_items(
	file_path: &std::path::Path,
	source: &str,
	level: Level,
) -> anyhow::Result<Vec<PublicItem>> {
	let lang = language_from_path(file_path);

	// 1. Rust — always use syn (most precise).
	if lang == Language::Rust {
		return rust::extract(source, level);
	}

	// 2. Tree-sitter — when the outline feature is enabled and a grammar is
	//    compiled in for this language, prefer tree-sitter extraction.
	#[cfg(feature = "outline")]
	if let Some(items) = ts_query::extract_with_ts(file_path, source, lang, level) {
		return Ok(items);
	}

	// 3. Regex fallback.
	Ok(text::extract(source, lang, level))
}
