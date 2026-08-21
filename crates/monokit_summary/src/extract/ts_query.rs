//! Tree-sitter outline query engine.
//!
//! Inspired by Zed's outline system and the tree-sitter tags convention, this
//! module uses tree-sitter queries with captures like `@definition.function`,
//! `@definition.class`, and `@name` to extract [`PublicItem`]s from source
//! files. Each supported language ships an outline query (`.scm`-style) that
//! captures the relevant syntactic nodes.

use std::path::Path;

#[cfg(feature = "outline")]
use tree_sitter::StreamingIterator;

use crate::types::ItemKind;
use crate::types::Language;
use crate::types::Level;
use crate::types::PublicItem;

// ----------------------------------------------------------------
// Outline queries per language
// ----------------------------------------------------------------
// These are tree-sitter query strings using the tags convention:
//   @definition.class    — class/struct/enum definition
//   @definition.function — function/method definition
//   @definition.method   — method definition (inside an impl/class)
//   @definition.interface — interface/trait definition
//   @definition.module   — module/namespace definition
//   @definition.constant — const/static definition
//   @definition.type     — type alias definition
//   @name                — the identifier name of the item
//   @doc                 — doc comment attached to the item
//
// The `@name` capture is always the **first** capture inside a definition
// pattern; we use it to pull the human-readable name.

/// Outline query for Rust.
#[cfg(feature = "lang-rust")]
const OUTLINE_RUST: &str = r"
(function_item name: (identifier) @name) @definition.function
(function_signature_item name: (identifier) @name) @definition.function
(struct_item name: (type_identifier) @name) @definition.class
(enum_item name: (type_identifier) @name) @definition.class
(trait_item name: (type_identifier) @name) @definition.interface
(impl_item type: (type_identifier) @name) @definition.class
(type_item name: (type_identifier) @name) @definition.type
(const_item name: (identifier) @name) @definition.constant
(static_item name: (identifier) @name) @definition.constant
(mod_item name: (identifier) @name) @definition.module
(macro_rules_item name: (identifier) @name) @definition.function
";

/// Outline query for TypeScript / TSX.
#[cfg(feature = "lang-typescript")]
const OUTLINE_TYPESCRIPT: &str = r"
(function_declaration name: (identifier) @name) @definition.function
(generator_function_declaration name: (identifier) @name) @definition.function
(class_declaration name: (type_identifier) @name) @definition.class
(interface_declaration name: (type_identifier) @name) @definition.interface
(type_alias_declaration name: (type_identifier) @name) @definition.type
(method_definition name: (property_identifier) @name) @definition.method
(enum_declaration name: (identifier) @name) @definition.class
";

/// Outline query for Python.
#[cfg(feature = "lang-python")]
const OUTLINE_PYTHON: &str = r"
(function_definition name: (identifier) @name) @definition.function
(class_definition name: (identifier) @name) @definition.class
";

/// Outline query for Go.
#[cfg(feature = "lang-go")]
const OUTLINE_GO: &str = r"
(function_declaration name: (identifier) @name) @definition.function
(method_declaration name: (field_identifier) @name) @definition.method
";

/// Outline query for Java.
#[cfg(feature = "lang-java")]
const OUTLINE_JAVA: &str = r"
(class_declaration name: (identifier) @name) @definition.class
(interface_declaration name: (identifier) @name) @definition.interface
(enum_declaration name: (identifier) @name) @definition.class
(method_declaration name: (identifier) @name) @definition.method
(constructor_declaration name: (identifier) @name) @definition.method
";

/// Outline query for C.
#[cfg(feature = "lang-c")]
const OUTLINE_C: &str = r"
(function_definition declarator: (function_declarator declarator: (identifier) @name)) @definition.function
(struct_specifier name: (type_identifier) @name) @definition.class
(enum_specifier name: (type_identifier) @name) @definition.class
";

/// Outline query for C++.
#[cfg(feature = "lang-cpp")]
const OUTLINE_CPP: &str = r"
(function_definition declarator: (function_declarator declarator: (identifier) @name)) @definition.function
(class_specifier name: (type_identifier) @name) @definition.class
(struct_specifier name: (type_identifier) @name) @definition.class
(enum_specifier name: (type_identifier) @name) @definition.class
(namespace_definition name: (identifier) @name) @definition.module
";

/// Outline query for C#.
#[cfg(feature = "lang-c-sharp")]
const OUTLINE_CSHARP: &str = r"
(class_declaration name: (identifier) @name) @definition.class
(interface_declaration name: (identifier) @name) @definition.interface
(struct_declaration name: (identifier) @name) @definition.class
(enum_declaration name: (identifier) @name) @definition.class
(method_declaration name: (identifier) @name) @definition.method
(namespace_declaration name: (identifier) @name) @definition.module
";

/// Outline query for Ruby.
#[cfg(feature = "lang-ruby")]
const OUTLINE_RUBY: &str = r"
(method name: (identifier) @name) @definition.function
(singleton_method name: (identifier) @name) @definition.method
(class name: (constant) @name) @definition.class
(module name: (constant) @name) @definition.module
";

/// Outline query for JSON (keys only).
#[cfg(feature = "lang-json")]
const OUTLINE_JSON: &str = r"
(pair key: (string (string_content) @name)) @definition.constant
";

// ----------------------------------------------------------------
// Language → outline query mapping
// ----------------------------------------------------------------

/// Returns the outline query string for the given language, if a tree-sitter
/// grammar is compiled in.
fn outline_query_for(lang: Language) -> Option<&'static str> {
	match lang {
		#[cfg(feature = "lang-rust")]
		Language::Rust => Some(OUTLINE_RUST),
		#[cfg(feature = "lang-typescript")]
		Language::TypeScript | Language::JavaScript => Some(OUTLINE_TYPESCRIPT),
		#[cfg(feature = "lang-python")]
		Language::Python => Some(OUTLINE_PYTHON),
		#[cfg(feature = "lang-go")]
		Language::Go => Some(OUTLINE_GO),
		#[cfg(feature = "lang-java")]
		Language::Java => Some(OUTLINE_JAVA),
		#[cfg(feature = "lang-c")]
		Language::C => Some(OUTLINE_C),
		#[cfg(feature = "lang-cpp")]
		Language::Cpp => Some(OUTLINE_CPP),
		#[cfg(feature = "lang-c-sharp")]
		Language::CSharp => Some(OUTLINE_CSHARP),
		#[cfg(feature = "lang-ruby")]
		Language::Ruby => Some(OUTLINE_RUBY),
		#[cfg(feature = "lang-json")]
		Language::Json => Some(OUTLINE_JSON),
		_ => None,
	}
}

/// Returns the tree-sitter [`Language`](tree_sitter::Language) for the given
/// language, if the corresponding grammar crate is compiled in.
fn ts_language_for(lang: Language) -> Option<tree_sitter::Language> {
	match lang {
		#[cfg(feature = "lang-rust")]
		Language::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
		#[cfg(feature = "lang-typescript")]
		Language::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
		#[cfg(feature = "lang-typescript")]
		Language::JavaScript => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
		#[cfg(feature = "lang-python")]
		Language::Python => Some(tree_sitter_python::LANGUAGE.into()),
		#[cfg(feature = "lang-go")]
		Language::Go => Some(tree_sitter_go::LANGUAGE.into()),
		#[cfg(feature = "lang-java")]
		Language::Java => Some(tree_sitter_java::LANGUAGE.into()),
		#[cfg(feature = "lang-c")]
		Language::C => Some(tree_sitter_c::LANGUAGE.into()),
		#[cfg(feature = "lang-cpp")]
		Language::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),
		#[cfg(feature = "lang-c-sharp")]
		Language::CSharp => Some(tree_sitter_c_sharp::LANGUAGE.into()),
		#[cfg(feature = "lang-ruby")]
		Language::Ruby => Some(tree_sitter_ruby::LANGUAGE.into()),
		#[cfg(feature = "lang-json")]
		Language::Json => Some(tree_sitter_json::LANGUAGE.into()),
		_ => None,
	}
}

// ----------------------------------------------------------------
// Capture name → ItemKind mapping
// ----------------------------------------------------------------

/// Map a tree-sitter query capture name (e.g. `"definition.function"`) to our
/// [`ItemKind`].
fn kind_from_capture(capture: &str) -> ItemKind {
	match capture {
		"definition.method" | "definition.function" => ItemKind::Function,
		"definition.class" => ItemKind::Class,
		"definition.interface" => ItemKind::Interface,
		"definition.module" => ItemKind::Module,
		"definition.type" => ItemKind::TypeAlias,
		"definition.constant" => ItemKind::Const,
		_ => ItemKind::Other(capture.to_string()),
	}
}

// ----------------------------------------------------------------
// Extraction entry point
// ----------------------------------------------------------------

/// Parse source code with the appropriate tree-sitter grammar and run the
/// outline query to extract [`PublicItem`]s.
///
/// Returns `None` if no grammar is compiled in for the given language.
#[cfg(feature = "outline")]
pub(crate) fn extract_with_ts(
	_file_path: &Path,
	source: &str,
	lang: Language,
	level: Level,
) -> Option<Vec<PublicItem>> {
	let ts_lang = ts_language_for(lang)?;
	let query_str = outline_query_for(lang)?;

	let mut parser = tree_sitter::Parser::new();
	parser.set_language(&ts_lang).ok()?;
	let tree = parser.parse(source, None)?;
	let root = tree.root_node();

	let query = tree_sitter::Query::new(&ts_lang, query_str).ok()?;
	let mut cursor = tree_sitter::QueryCursor::new();

	let mut items = Vec::new();
	let source_bytes = source.as_bytes();

	// Iterate over query matches. Each match groups one @definition.* capture
	// with its @name sibling. We use QueryCursor::matches() which yields
	// QueryMatch items via StreamingIterator.
	let mut matches_iter = cursor.matches(&query, root, source_bytes);

	while let Some(match_ref) = matches_iter.next() {
		// Find the definition capture and name capture in this match.
		let mut def_capture: Option<tree_sitter::QueryCapture<'_>> = None;
		let mut name_text: Option<String> = None;

		for capture in match_ref.captures {
			let capture_name = &query.capture_names()[capture.index as usize];
			if capture_name.starts_with("definition.") && def_capture.is_none() {
				def_capture = Some(*capture);
			} else if *capture_name == "name" {
				name_text = Some(
					capture
						.node
						.utf8_text(source_bytes)
						.unwrap_or_default()
						.to_string(),
				);
			}
		}

		let Some(def) = def_capture else {
			continue;
		};

		let name = match name_text {
			Some(n) if !n.is_empty() => n,
			_ => continue,
		};

		let capture_name = &query.capture_names()[def.index as usize];
		let kind = kind_from_capture(capture_name);
		let def_node = def.node;
		let line = def_node.start_position().row as u32 + 1;

		// Collect doc comments immediately before the definition node.
		let (first_doc_line, full_docs) = extract_docs(source, def_node, level);

		let signature = if level >= Level::Signature {
			Some(
				def_node
					.utf8_text(source_bytes)
					.unwrap_or_default()
					.to_string(),
			)
		} else {
			None
		};

		items.push(PublicItem {
			name,
			kind,
			line,
			signature,
			first_doc_line,
			full_docs,
			parent_path: Vec::new(),
		});
	}

	Some(items)
}

// ---------------------------------------------------------------------------
// Doc comment extraction
// ---------------------------------------------------------------------------

/// Extract doc comments immediately before a tree-sitter node.
///
/// Walks backwards from the node's start line to collect consecutive comment
/// lines. Returns `(first_line, full_docs)`.
fn extract_docs(
	source: &str,
	node: tree_sitter::Node,
	level: Level,
) -> (Option<String>, Option<String>) {
	let start_row = node.start_position().row;
	if start_row == 0 {
		return (None, None);
	}

	let lines: Vec<&str> = source.lines().collect();
	let mut doc_lines: Vec<&str> = Vec::new();
	let mut row = start_row;

	// Walk backwards collecting comment lines.
	while row > 0 {
		row -= 1;
		let line = lines.get(row).map_or("", |l| l.trim());
		if line.starts_with("///")
			|| line.starts_with("//!")
			|| line.starts_with("/**")
			|| line.starts_with("/*!")
			|| line.starts_with("/*")
			|| line.starts_with("//")
		{
			doc_lines.push(line);
		} else if line.is_empty() {
			// Skip blank lines between docs and definition.
		} else {
			break;
		}
	}

	if doc_lines.is_empty() {
		return (None, None);
	}

	doc_lines.reverse();

	let first = doc_lines.first().map(|s| {
		// Strip comment prefix for a cleaner first line.
		s.trim_start_matches('/')
			.trim_start_matches('*')
			.trim_start_matches('!')
			.trim()
			.to_string()
	});

	let full = if level == Level::Outline {
		Some(doc_lines.join("\n"))
	} else {
		None
	};

	(first, full)
}
