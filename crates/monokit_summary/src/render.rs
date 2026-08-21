//! Human-readable and machine-readable rendering of summaries.
//!
//! The [`render`] function produces a token-efficient plain-text view that
//! an LLM can consume directly. The [`render_json`] function produces
//! structured JSON.

use std::fmt::Write;

use crate::types::Level;
use crate::types::Summary;

// ---------------------------------------------------------------------------
// Plain text
// ---------------------------------------------------------------------------

/// Render a [`Summary`] as plain text optimised for LLM context windows.
///
/// The output format varies by [`Level`]:
///
/// - **Index**: one line per file listing public symbol names.
/// - **Signature**: one line per item with name, kind, signature, and first
///   doc line.
/// - **Outline**: full detail per item, including all doc comments.
#[allow(clippy::format_push_string)]
pub(crate) fn render(summary: &Summary) -> String {
	let mut out = String::new();
	let level = summary.level;

	for file in &summary.files {
		let path = file.file_path.to_string_lossy();

		match level {
			Level::Index => {
				let names: Vec<&str> = file.items.iter().map(|i| i.name.as_str()).collect();
				if !names.is_empty() {
					writeln!(out, "{}: {}", path, names.join(", ")).unwrap();
				}
			}
			Level::Signature => {
				writeln!(out, "--- {path} ---").unwrap();
				for item in &file.items {
					write!(
						out,
						"L{:>4} {:>10} {}",
						item.line,
						format!("{:?}", item.kind),
						item.name,
					)
					.unwrap();
					if let Some(sig) = &item.signature {
						write!(out, " {sig}").unwrap();
					}
					if let Some(doc) = &item.first_doc_line {
						write!(out, " // {doc}").unwrap();
					}
					writeln!(out).unwrap();
				}
				writeln!(out).unwrap();
			}
			Level::Outline => {
				writeln!(out, "=== {path} ===").unwrap();
				for item in &file.items {
					if !item.parent_path.is_empty() {
						write!(out, "{}::", item.parent_path.join("::")).unwrap();
					}
					write!(
						out,
						"L{:>4} {:>10} {}",
						item.line,
						format!("{:?}", item.kind),
						item.name,
					)
					.unwrap();
					if let Some(sig) = &item.signature {
						write!(out, " {sig}").unwrap();
					}
					writeln!(out).unwrap();
					if let Some(doc) = &item.full_docs {
						for line in doc.lines() {
							writeln!(out, "    {line}").unwrap();
						}
					} else if let Some(doc) = &item.first_doc_line {
						writeln!(out, "    {doc}").unwrap();
					}
				}
				writeln!(out).unwrap();
			}
		}
	}

	out
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

/// Render a [`Summary`] as pretty-printed JSON (requires `serde` derive).
pub(crate) fn render_json(summary: &Summary) -> anyhow::Result<String> {
	Ok(serde_json::to_string_pretty(summary)?)
}
