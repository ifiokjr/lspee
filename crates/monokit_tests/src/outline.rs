//! Integration tests for `monokit_summary` — outline extraction.

use std::path::Path;

use monokit_summary::Level;
use monokit_summary::language_from_path;
use monokit_summary::summarise_file;

/// Read a fixture file from the `fixtures/` directory.
fn fixture(name: &str) -> String {
	let path = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("fixtures")
		.join(name);
	std::fs::read_to_string(&path)
		.unwrap_or_else(|e| panic!("failed to read fixture '{name}': {e}"))
}

/// Helper: summarise a single fixture file at the given level, rendering as
/// plain text.
fn summarise_fixture(name: &str, level: Level) -> String {
	let path = Path::new(name);
	let source = fixture(name);
	let file_summary =
		summarise_file(path, &source, level).expect("summarise_file should not fail");
	monokit_summary::render_file_text(&file_summary)
}

// ── Rust ────────────────────────────────────────────────────────────────────

#[test]
fn rust_index_snapshot() {
	let output = summarise_fixture("example.rs", Level::Index);
	insta::assert_snapshot!("rust_index", output);
}

#[test]
fn rust_signature_snapshot() {
	let output = summarise_fixture("example.rs", Level::Signature);
	insta::assert_snapshot!("rust_signature", output);
}

#[test]
fn rust_outline_snapshot() {
	let output = summarise_fixture("example.rs", Level::Outline);
	insta::assert_snapshot!("rust_outline", output);
}

// ── TypeScript ────────────────────────────────────────────────────────────────

#[test]
fn typescript_index_snapshot() {
	let output = summarise_fixture("example.ts", Level::Index);
	insta::assert_snapshot!("typescript_index", output);
}

#[test]
fn typescript_signature_snapshot() {
	let output = summarise_fixture("example.ts", Level::Signature);
	insta::assert_snapshot!("typescript_signature", output);
}

// ── Python ───────────────────────────────────────────────────────────────────

#[test]
fn python_index_snapshot() {
	let output = summarise_fixture("example.py", Level::Index);
	insta::assert_snapshot!("python_index", output);
}

#[test]
fn python_signature_snapshot() {
	let output = summarise_fixture("example.py", Level::Signature);
	insta::assert_snapshot!("python_signature", output);
}

// ── Go ────────────────────────────────────────────────────────────────────────

#[test]
fn go_index_snapshot() {
	let output = summarise_fixture("example.go", Level::Index);
	insta::assert_snapshot!("go_index", output);
}

#[test]
fn go_signature_snapshot() {
	let output = summarise_fixture("example.go", Level::Signature);
	insta::assert_snapshot!("go_signature", output);
}

// ── Java ──────────────────────────────────────────────────────────────────────

#[test]
fn java_index_snapshot() {
	let output = summarise_fixture("example.java", Level::Index);
	insta::assert_snapshot!("java_index", output);
}

#[test]
fn java_signature_snapshot() {
	let output = summarise_fixture("example.java", Level::Signature);
	insta::assert_snapshot!("java_signature", output);
}

// ── C ─────────────────────────────────────────────────────────────────────────

#[test]
fn c_index_snapshot() {
	let output = summarise_fixture("example.c", Level::Index);
	insta::assert_snapshot!("c_index", output);
}

// ── C++ ───────────────────────────────────────────────────────────────────────

#[test]
fn cpp_index_snapshot() {
	let output = summarise_fixture("example.cpp", Level::Index);
	insta::assert_snapshot!("cpp_index", output);
}

// ── C# ────────────────────────────────────────────────────────────────────────

#[test]
fn csharp_index_snapshot() {
	let output = summarise_fixture("example.cs", Level::Index);
	insta::assert_snapshot!("csharp_index", output);
}

// ── Ruby ───────────────────────────────────────────────────────────────────────

#[test]
fn ruby_index_snapshot() {
	let output = summarise_fixture("example.rb", Level::Index);
	insta::assert_snapshot!("ruby_index", output);
}

// ── JSON ──────────────────────────────────────────────────────────────────────

#[test]
fn json_index_snapshot() {
	let output = summarise_fixture("example.json", Level::Index);
	insta::assert_snapshot!("json_index", output);
}

// ── Language detection ────────────────────────────────────────────────────────

#[test]
fn language_detection_common_extensions() {
	assert!(matches!(
		language_from_path(Path::new("src/main.rs")),
		monokit_summary::Language::Rust
	));
	assert!(matches!(
		language_from_path(Path::new("app.tsx")),
		monokit_summary::Language::TypeScript
	));
	assert!(matches!(
		language_from_path(Path::new("main.go")),
		monokit_summary::Language::Go
	));
	assert!(matches!(
		language_from_path(Path::new("app.py")),
		monokit_summary::Language::Python
	));
}
