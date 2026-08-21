//! Regex-based extractor for non-Rust languages.
//!
//! Uses pattern matching to find public/exported items in TypeScript/JavaScript,
//! Go, Python, and other languages. This is intentionally lightweight —
//! it trades AST precision for breadth of language coverage.

use crate::types::ItemKind;
use crate::types::Language;
use crate::types::Level;
use crate::types::PublicItem;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub(super) fn extract(source: &str, lang: Language, level: Level) -> Vec<PublicItem> {
	match lang {
		Language::TypeScript | Language::JavaScript => extract_ts_js(source, level),
		Language::Go => extract_go(source, level),
		Language::Python => extract_python(source, level),
		Language::Java | Language::Kotlin | Language::Dart => {
			extract_java_family(source, lang, level)
		}
		Language::CSharp => extract_csharp(source, level),
		Language::Swift => extract_swift(source, level),
		Language::Zig => extract_zig(source, level),
		Language::C | Language::Cpp | Language::ObjectiveC => extract_c_family(source, lang, level),
		Language::Scala
		| Language::Haskell
		| Language::Elm
		| Language::OCaml
		| Language::FSharp
		| Language::Elixir
		| Language::Gleam
		| Language::Nim
		| Language::Julia
		| Language::Ruby
		| Language::Lua
		| Language::Perl
		| Language::Php
		| Language::Shell
		| Language::Rust
		| Language::Html
		| Language::Css
		| Language::Svelte
		| Language::Vue
		| Language::Toml
		| Language::Yaml
		| Language::Json
		| Language::Protobuf
		| Language::Markdown
		| Language::Solidity
		| Language::Nix
		| Language::R
		| Language::Unknown => extract_generic(source, level),
	}
}

// ---------------------------------------------------------------------------
// Line number helper
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// TypeScript / JavaScript
// ---------------------------------------------------------------------------

fn extract_ts_js(source: &str, level: Level) -> Vec<PublicItem> {
	let mut items = Vec::new();

	// We look for:
	//  - `export function name(` or `function name(` (TS: with types)
	//  - `export const name =` or `export let name =`
	//  - `export class Name`
	//  - `export interface Name`
	//  - `export type Name`
	//  - `export enum Name`
	//  - `export default function name(`
	//  JSDoc: /** ... */ immediately before an export
	for (i, line) in source.lines().enumerate() {
		let trimmed = line.trim();
		let line_num = (i + 1) as u32;
		let sig = if level >= Level::Signature {
			Some(
				trimmed
					.trim_end_matches('{')
					.trim_end_matches(':')
					.trim()
					.to_string(),
			)
		} else {
			None
		};

		// export function / async function
		if let Some(rest) = trimmed
			.strip_prefix("export function ")
			.or_else(|| trimmed.strip_prefix("export async function "))
			.or_else(|| trimmed.strip_prefix("function "))
			.or_else(|| trimmed.strip_prefix("async function "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Function,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
				continue;
			}
		}

		// export class
		if let Some(rest) = trimmed
			.strip_prefix("export class ")
			.or_else(|| trimmed.strip_prefix("class "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Class,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
				continue;
			}
		}

		// export interface
		if let Some(rest) = trimmed
			.strip_prefix("export interface ")
			.or_else(|| trimmed.strip_prefix("interface "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Interface,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
				continue;
			}
		}

		// export type
		if let Some(rest) = trimmed
			.strip_prefix("export type ")
			.or_else(|| trimmed.strip_prefix("type "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::TypeAlias,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
				continue;
			}
		}

		// export enum
		if let Some(rest) = trimmed
			.strip_prefix("export enum ")
			.or_else(|| trimmed.strip_prefix("enum "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Enum,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
				continue;
			}
		}

		// export const / export let / export var
		if let Some(rest) = trimmed
			.strip_prefix("export const ")
			.or_else(|| trimmed.strip_prefix("export let "))
			.or_else(|| trimmed.strip_prefix("export var "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Var,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
			}
		}
	}

	items
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

fn extract_go(source: &str, level: Level) -> Vec<PublicItem> {
	let mut items = Vec::new();

	for (i, line) in source.lines().enumerate() {
		let trimmed = line.trim();
		let line_num = (i + 1) as u32;

		// Go exports are determined by capitalisation, not keywords.
		// We match `func`, `type`, `const`, `var` followed by an
		// exported (uppercase-starting) identifier.
		let sig = if level >= Level::Signature {
			Some(trimmed.trim_end_matches('{').trim().to_string())
		} else {
			None
		};

		// func (...) Name(
		if let Some(rest) = trimmed.strip_prefix("func ") {
			// Could be a method receiver: `func (r Receiver) Name(`
			// or a package-level function: `func Name(`
			let name = if let Some(stripped) = rest.strip_prefix('(') {
				// Method on receiver — skip the receiver part.
				// Find closing ')' then extract name.
				if let Some(close) = stripped.find(") ") {
					let after_recv = &stripped[close + 2..];
					extract_identifier(after_recv).to_string()
				} else if let Some(close) = stripped.find(')') {
					let after_recv = &stripped[close + 1..].trim_start();
					extract_identifier(after_recv).to_string()
				} else {
					String::new()
				}
			} else {
				extract_identifier(rest).to_string()
			};

			if !name.is_empty() && name.starts_with(char::is_uppercase) {
				items.push(PublicItem {
					name,
					kind: ItemKind::Function,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: go_doc_before(source, i),
					full_docs: if level == Level::Outline {
						go_full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
			}
		}

		// type Name struct / interface
		if let Some(rest) = trimmed.strip_prefix("type ") {
			let name = extract_identifier(rest);
			if !name.is_empty() && name.starts_with(char::is_uppercase) {
				let kind = if rest.contains("struct") {
					ItemKind::Struct
				} else if rest.contains("interface") {
					ItemKind::Interface
				} else {
					ItemKind::TypeAlias
				};
				items.push(PublicItem {
					name: name.to_string(),
					kind,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: go_doc_before(source, i),
					full_docs: if level == Level::Outline {
						go_full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
			}
		}

		// const / var with exported names
		if let Some(rest) = trimmed
			.strip_prefix("const ")
			.or_else(|| trimmed.strip_prefix("var "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() && name.starts_with(char::is_uppercase) {
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Const,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: go_doc_before(source, i),
					full_docs: if level == Level::Outline {
						go_full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
			}
		}
	}

	items
}

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

fn extract_python(source: &str, level: Level) -> Vec<PublicItem> {
	let mut items = Vec::new();

	for (i, line) in source.lines().enumerate() {
		let trimmed = line.trim();
		let line_num = (i + 1) as u32;

		// def name(
		if let Some(rest) = trimmed.strip_prefix("def ") {
			let name = extract_identifier(rest);
			if !name.is_empty() {
				// In Python, functions not starting with _ are conventionally
				// public. Check for `__` dunder names and include them too.
				let sig = if level >= Level::Signature {
					Some(trimmed.trim_end_matches(':').trim().to_string())
				} else {
					None
				};
				// Python docstrings are on the line after the def.
				let first_doc = py_doc_after(source, i);
				let full_docs = if level == Level::Outline {
					py_full_doc_after(source, i)
				} else {
					None
				};
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Function,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: first_doc,
					full_docs,
					parent_path: Vec::new(),
				});
			}
		}

		// class Name
		if let Some(rest) = trimmed.strip_prefix("class ") {
			let name = extract_identifier(rest);
			if !name.is_empty() {
				let sig = if level >= Level::Signature {
					Some(
						trimmed
							.trim_end_matches(':')
							.trim_end_matches('(')
							.trim()
							.to_string(),
					)
				} else {
					None
				};
				let first_doc = py_doc_after(source, i);
				let full_docs = if level == Level::Outline {
					py_full_doc_after(source, i)
				} else {
					None
				};
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Class,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: first_doc,
					full_docs,
					parent_path: Vec::new(),
				});
			}
		}
	}

	items
}

// ---------------------------------------------------------------------------
// Generic fallback — best-effort function / class detection
// ---------------------------------------------------------------------------

fn extract_generic(source: &str, level: Level) -> Vec<PublicItem> {
	let mut items = Vec::new();

	for (i, line) in source.lines().enumerate() {
		let trimmed = line.trim();
		let line_num = (i + 1) as u32;

		// C/C++: `type name(`  (function declarations)
		// Java/Kotlin/C#: `public ... name(`
		// Zig: `pub fn name(`
		if trimmed.contains('(')
			&& (trimmed.starts_with("pub ")
				|| trimmed.starts_with("public ")
				|| trimmed.starts_with("export ")
				|| trimmed.starts_with("fn ")
				|| trimmed.starts_with("pub fn "))
		{
			let name = extract_name_from_line(trimmed);
			if !name.is_empty() {
				let sig = if level >= Level::Signature {
					Some(
						trimmed
							.trim_end_matches('{')
							.trim_end_matches(';')
							.trim()
							.to_string(),
					)
				} else {
					None
				};
				items.push(PublicItem {
					name,
					kind: ItemKind::Function,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: None,
					full_docs: None,
					parent_path: Vec::new(),
				});
			}
		}
	}

	items
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a Rust-like identifier (alphanumeric + `_`) from the start of a
/// string, stopping at the first character that isn't part of an ident.
fn extract_identifier(s: &str) -> &str {
	let end = s
		.find(|c: char| !c.is_alphanumeric() && c != '_')
		.unwrap_or(s.len());
	&s[..end]
}

/// Extract a "name" from the start of a line by heuristics.
/// Strips common prefixes like visibility modifiers, then grabs the
/// first identifier-like token.
fn extract_name_from_line(line: &str) -> String {
	// Strip common visibility/qualifier prefixes.
	let s = line
		.strip_prefix("pub fn ")
		.or_else(|| line.strip_prefix("pub async fn "))
		.or_else(|| line.strip_prefix("pub "))
		.or_else(|| line.strip_prefix("public "))
		.or_else(|| line.strip_prefix("export "))
		.or_else(|| line.strip_prefix("fn "))
		.or_else(|| line.strip_prefix("async fn "))
		.unwrap_or(line);

	extract_identifier(s).to_string()
}

/// `JSDoc` / `/**` line immediately before the current line.
fn doc_line_before(source: &str, line_idx: usize) -> Option<String> {
	let lines: Vec<&str> = source.lines().collect();
	let mut i = line_idx;
	// Skip blank lines.
	while i > 0 && lines[i - 1].trim().is_empty() {
		i -= 1;
	}
	if i > 0 {
		let prev = lines[i - 1].trim();
		if prev.starts_with("*/") || prev.starts_with("* ") || prev.starts_with("/**") {
			// Find the first content line of the doc comment.
			let content = prev
				.trim_start_matches("/**")
				.trim_start_matches("/*")
				.trim_start_matches("* ")
				.trim_start_matches('*')
				.trim_end_matches("*/")
				.trim();
			if !content.is_empty() {
				return Some(content.to_string());
			}
		}
	}
	None
}

fn full_doc_before(source: &str, line_idx: usize) -> Option<String> {
	let lines: Vec<&str> = source.lines().collect();
	let mut i = line_idx;
	while i > 0 && lines[i - 1].trim().is_empty() {
		i -= 1;
	}
	if i > 0 {
		let prev = lines[i - 1].trim();
		if prev.starts_with("*/") || prev.starts_with("* ") || prev.starts_with("/**") {
			// Walk backwards to find start of comment.
			let mut start = i - 1;
			while start > 0 && !lines[start - 1].trim().starts_with("/**") {
				start -= 1;
			}
			let doc_lines: Vec<&str> = lines[start..i]
				.iter()
				.map(|l| {
					l.trim()
						.trim_start_matches("/**")
						.trim_start_matches("* ")
						.trim_start_matches('*')
						.trim_end_matches("*/")
						.trim()
				})
				.filter(|l| !l.is_empty())
				.collect();
			if !doc_lines.is_empty() {
				return Some(doc_lines.join("\n"));
			}
		}
	}
	None
}

/// Go doc comments: `//` lines immediately before the declaration.
fn go_doc_before(source: &str, line_idx: usize) -> Option<String> {
	let lines: Vec<&str> = source.lines().collect();
	// Walk backwards over `//` comment lines.
	let mut i = line_idx;
	while i > 0 {
		let prev = lines[i - 1].trim();
		if prev.starts_with("//") || prev.is_empty() {
			i -= 1;
		} else {
			break;
		}
	}
	if i < line_idx {
		let first = lines[i].trim().trim_start_matches("//").trim();
		if !first.is_empty() {
			return Some(first.to_string());
		}
	}
	None
}

fn go_full_doc_before(source: &str, line_idx: usize) -> Option<String> {
	let lines: Vec<&str> = source.lines().collect();
	let mut i = line_idx;
	while i > 0 {
		let prev = lines[i - 1].trim();
		if prev.starts_with("//") || prev.is_empty() {
			i -= 1;
		} else {
			break;
		}
	}
	if i < line_idx {
		let doc_lines: Vec<String> = lines[i..line_idx]
			.iter()
			.map(|l| l.trim().trim_start_matches("//").trim().to_string())
			.filter(|l| !l.is_empty())
			.collect();
		if !doc_lines.is_empty() {
			return Some(doc_lines.join("\n"));
		}
	}
	None
}

/// Python docstring on the line after a def/class.
fn py_doc_after(source: &str, line_idx: usize) -> Option<String> {
	let lines: Vec<&str> = source.lines().collect();
	if line_idx + 1 < lines.len() {
		let next = lines[line_idx + 1].trim();
		let stripped = next
			.trim_start_matches("\"\"\"")
			.trim_start_matches("'''")
			.trim_end_matches("\"\"\"")
			.trim_end_matches("'''")
			.trim();
		if !stripped.is_empty() {
			return Some(stripped.to_string());
		}
	}
	None
}

fn py_full_doc_after(source: &str, line_idx: usize) -> Option<String> {
	let lines: Vec<&str> = source.lines().collect();
	if line_idx + 1 >= lines.len() {
		return None;
	}
	let first = lines[line_idx + 1].trim();
	let (open, close) = if first.starts_with("\"\"\"") {
		("\"\"\"", "\"\"\"")
	} else if first.starts_with("'''") {
		("'''", "'''")
	} else {
		return None;
	};

	let content_start = first.trim_start_matches(open).trim();
	// Check single-line docstring.
	if content_start.ends_with(close) {
		let inner = content_start.trim_end_matches(close).trim();
		if !inner.is_empty() {
			return Some(inner.to_string());
		}
		return None;
	}

	// Multi-line docstring.
	let mut collected = Vec::new();
	if !content_start.is_empty() {
		collected.push(content_start.to_string());
	}
	for line in lines.iter().take(lines.len()).skip(line_idx + 2) {
		let trimmed = line.trim();
		if trimmed == close {
			break;
		}
		collected.push(trimmed.to_string());
	}
	if !collected.is_empty() {
		return Some(collected.join("\n").trim().to_string());
	}
	None
}

// ---------------------------------------------------------------------------
// Java / Kotlin / Dart (Java-family syntax)
// ---------------------------------------------------------------------------

fn extract_java_family(source: &str, lang: Language, level: Level) -> Vec<PublicItem> {
	let mut items = Vec::new();

	for (i, line) in source.lines().enumerate() {
		let trimmed = line.trim();
		let line_num = (i + 1) as u32;

		// Strip common access modifiers.
		let stripped = trimmed
			.strip_prefix("public ")
			.or_else(|| trimmed.strip_prefix("private "))
			.or_else(|| trimmed.strip_prefix("protected "))
			.or_else(|| trimmed.strip_prefix("internal "))
			.unwrap_or(trimmed);

		let sig = if level >= Level::Signature {
			Some(
				trimmed
					.trim_end_matches('{')
					.trim_end_matches(';')
					.trim()
					.to_string(),
			)
		} else {
			None
		};

		// class Name / enum Name / interface Name / object Name (Kotlin)
		if let Some(rest) = stripped
			.strip_prefix("class ")
			.or_else(|| stripped.strip_prefix("enum class "))
			.or_else(|| stripped.strip_prefix("interface "))
			.or_else(|| stripped.strip_prefix("enum "))
			.or_else(|| stripped.strip_prefix("object "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				let kind = if stripped.starts_with("interface") {
					ItemKind::Interface
				} else if stripped.starts_with("enum") {
					ItemKind::Enum
				} else {
					ItemKind::Class
				};
				items.push(PublicItem {
					name: name.to_string(),
					kind,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
				continue;
			}
		}

		// fun Name( (Kotlin) / def Name( (Dart)
		if let Some(rest) = stripped
			.strip_prefix("fun ")
			.or_else(|| stripped.strip_prefix("async fun "))
			.or_else(|| stripped.strip_prefix("def "))
			.or_else(|| trimmed.strip_prefix("public fun "))
			.or_else(|| trimmed.strip_prefix("private fun "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Function,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
				continue;
			}
		}

		// val / var (Kotlin)
		if let Some(rest) = stripped
			.strip_prefix("val ")
			.or_else(|| stripped.strip_prefix("var "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Var,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
			}
		}
	}

	// Suppress unused-variable warning for lang
	let _ = lang;
	items
}

// ---------------------------------------------------------------------------
// C#
// ---------------------------------------------------------------------------

fn extract_csharp(source: &str, level: Level) -> Vec<PublicItem> {
	let mut items = Vec::new();

	for (i, line) in source.lines().enumerate() {
		let trimmed = line.trim();
		let line_num = (i + 1) as u32;

		// Only public/internal items
		if !trimmed.starts_with("public ")
			&& !trimmed.starts_with("internal ")
			&& !trimmed.starts_with("static ")
		{
			continue;
		}

		let sig = if level >= Level::Signature {
			Some(
				trimmed
					.trim_end_matches('{')
					.trim_end_matches(';')
					.trim()
					.to_string(),
			)
		} else {
			None
		};

		let stripped = trimmed
			.strip_prefix("public ")
			.or_else(|| trimmed.strip_prefix("internal "))
			.or_else(|| trimmed.strip_prefix("static "))
			.unwrap_or(trimmed);

		if let Some(rest) = stripped
			.strip_prefix("class ")
			.or_else(|| stripped.strip_prefix("struct "))
			.or_else(|| stripped.strip_prefix("interface "))
			.or_else(|| stripped.strip_prefix("enum "))
			.or_else(|| stripped.strip_prefix("record "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				let kind = if stripped.starts_with("interface") {
					ItemKind::Interface
				} else if stripped.starts_with("enum") {
					ItemKind::Enum
				} else {
					ItemKind::Class
				};
				items.push(PublicItem {
					name: name.to_string(),
					kind,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
				continue;
			}
		}

		if let Some(rest) = stripped
			.strip_prefix("void ")
			.or_else(|| stripped.strip_prefix("async void "))
			.or_else(|| stripped.strip_prefix("async Task"))
			.or_else(|| stripped.strip_prefix("Task"))
		{
			let after = rest.trim_start();
			let name = extract_identifier(after);
			if !name.is_empty() {
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Function,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
			}
		}
	}

	items
}

// ---------------------------------------------------------------------------
// Swift
// ---------------------------------------------------------------------------

fn extract_swift(source: &str, level: Level) -> Vec<PublicItem> {
	let mut items = Vec::new();

	for (i, line) in source.lines().enumerate() {
		let trimmed = line.trim();
		let line_num = (i + 1) as u32;

		let sig = if level >= Level::Signature {
			Some(
				trimmed
					.trim_end_matches('{')
					.trim_end_matches(':')
					.trim()
					.to_string(),
			)
		} else {
			None
		};

		if let Some(rest) = trimmed
			.strip_prefix("func ")
			.or_else(|| trimmed.strip_prefix("public func "))
			.or_else(|| trimmed.strip_prefix("private func "))
			.or_else(|| trimmed.strip_prefix("internal func "))
			.or_else(|| trimmed.strip_prefix("static func "))
			.or_else(|| trimmed.strip_prefix("class func "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Function,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
				continue;
			}
		}

		if let Some(rest) = trimmed
			.strip_prefix("struct ")
			.or_else(|| trimmed.strip_prefix("class "))
			.or_else(|| trimmed.strip_prefix("enum "))
			.or_else(|| trimmed.strip_prefix("protocol "))
			.or_else(|| trimmed.strip_prefix("actor "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				let kind = if trimmed.starts_with("protocol") {
					ItemKind::Interface
				} else if trimmed.starts_with("enum") {
					ItemKind::Enum
				} else if trimmed.starts_with("struct") {
					ItemKind::Struct
				} else {
					ItemKind::Class
				};
				items.push(PublicItem {
					name: name.to_string(),
					kind,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
			}
		}
	}

	items
}

// ---------------------------------------------------------------------------
// Zig
// ---------------------------------------------------------------------------

fn extract_zig(source: &str, level: Level) -> Vec<PublicItem> {
	let mut items = Vec::new();

	for (i, line) in source.lines().enumerate() {
		let trimmed = line.trim();
		let line_num = (i + 1) as u32;

		let sig = if level >= Level::Signature {
			Some(trimmed.trim_end_matches('{').trim().to_string())
		} else {
			None
		};

		// pub fn Name / fn Name (Zig doesn't have access modifiers — everything is file-private by default)
		if let Some(rest) = trimmed
			.strip_prefix("pub fn ")
			.or_else(|| trimmed.strip_prefix("fn "))
			.or_else(|| trimmed.strip_prefix("pub extern fn "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				items.push(PublicItem {
					name: name.to_string(),
					kind: ItemKind::Function,
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
				continue;
			}
		}

		// pub const Name / var Name
		if let Some(rest) = trimmed
			.strip_prefix("pub const ")
			.or_else(|| trimmed.strip_prefix("const "))
			.or_else(|| trimmed.strip_prefix("pub var "))
			.or_else(|| trimmed.strip_prefix("var "))
		{
			let name = extract_identifier(rest);
			if !name.is_empty() {
				items.push(PublicItem {
					name: name.to_string(),
					kind: if trimmed.contains("const") {
						ItemKind::Const
					} else {
						ItemKind::Var
					},
					line: line_num,
					signature: sig.clone(),
					first_doc_line: doc_line_before(source, i),
					full_docs: if level == Level::Outline {
						full_doc_before(source, i)
					} else {
						None
					},
					parent_path: Vec::new(),
				});
			}
		}
	}

	items
}

// ---------------------------------------------------------------------------
// C / C++ / Objective-C
// ---------------------------------------------------------------------------

fn extract_c_family(source: &str, lang: Language, level: Level) -> Vec<PublicItem> {
	let mut items = Vec::new();

	for (i, line) in source.lines().enumerate() {
		let trimmed = line.trim();
		let line_num = (i + 1) as u32;

		// C/C++: function declarations often don't have `pub` but we pick
		// up top-level function-like lines with `(` and `)`.
		// Objective-C: `@interface`, `@implementation`, `- (ret) name`, `+ (ret) name`
		if lang == Language::ObjectiveC {
			if let Some(rest) = trimmed.strip_prefix("@interface ") {
				let name = extract_identifier(rest);
				if !name.is_empty() {
					let sig = if level >= Level::Signature {
						Some(trimmed.trim_end_matches('{').trim().to_string())
					} else {
						None
					};
					items.push(PublicItem {
						name: name.to_string(),
						kind: ItemKind::Interface,
						line: line_num,
						signature: sig,
						first_doc_line: doc_line_before(source, i),
						full_docs: if level == Level::Outline {
							full_doc_before(source, i)
						} else {
							None
						},
						parent_path: Vec::new(),
					});
					continue;
				}
			}
			if let Some(rest) = trimmed.strip_prefix("@implementation ") {
				let name = extract_identifier(rest);
				if !name.is_empty() {
					let sig = if level >= Level::Signature {
						Some(trimmed.trim_end_matches('{').trim().to_string())
					} else {
						None
					};
					items.push(PublicItem {
						name: name.to_string(),
						kind: ItemKind::Class,
						line: line_num,
						signature: sig,
						first_doc_line: doc_line_before(source, i),
						full_docs: if level == Level::Outline {
							full_doc_before(source, i)
						} else {
							None
						},
						parent_path: Vec::new(),
					});
					continue;
				}
			}
		}

		// Use the generic extractor for function/class-like lines
		// (delegates to extract_name_from_line for C/C++)
		if trimmed.contains('(')
			&& (trimmed.starts_with("pub ")
				|| trimmed.starts_with("public ")
				|| trimmed.starts_with("export ")
				|| trimmed.starts_with("fn ")
				|| trimmed.starts_with("pub fn ")
				|| trimmed.starts_with("void ")
				|| trimmed.starts_with("int ")
				|| trimmed.starts_with("char ")
				|| trimmed.starts_with("bool ")
				|| trimmed.starts_with("float ")
				|| trimmed.starts_with("double ")
				|| trimmed.starts_with("auto ")
				|| trimmed.starts_with("static ")
				|| trimmed.starts_with("inline "))
		{
			let name = extract_name_from_line(trimmed);
			if !name.is_empty() {
				let sig = if level >= Level::Signature {
					Some(
						trimmed
							.trim_end_matches('{')
							.trim_end_matches(';')
							.trim()
							.to_string(),
					)
				} else {
					None
				};
				items.push(PublicItem {
					name,
					kind: ItemKind::Function,
					line: line_num,
					signature: sig,
					first_doc_line: None,
					full_docs: None,
					parent_path: Vec::new(),
				});
			}
		}
	}

	items
}
