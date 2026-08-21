//! Integration tests for `monokit_summary` — full summary rendering.

use std::path::Path;

use monokit_summary::Level;
use monokit_summary::render_file_json;
use monokit_summary::render_file_text;
use monokit_summary::summarise_file;

/// Read a fixture file from the `fixtures/` directory.
fn fixture(name: &str) -> String {
	let path = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("fixtures")
		.join(name);
	std::fs::read_to_string(&path)
		.unwrap_or_else(|e| panic!("failed to read fixture '{name}': {e}"))
}

#[test]
fn rust_json_output_snapshot() {
	let path = Path::new("example.rs");
	let source = fixture("example.rs");
	let file_summary = summarise_file(path, &source, Level::Signature).expect("summarise_file");
	let json = render_file_json(&file_summary).expect("render_file_json");
	insta::assert_snapshot!("rust_json", json);
}

// ── Compression ratio tests ───────────────────────────────────────────────

/// Compute the byte-size ratio of summary vs original source.
fn compression_ratio(source: &str, rendered: &str) -> f64 {
	if source.is_empty() {
		return 0.0;
	}
	rendered.len() as f64 / source.len() as f64 * 100.0
}

#[test]
fn index_level_shrinks_rust_significantly() {
	let source = fixture("example.rs");
	let path = Path::new("example.rs");
	let file_summary = summarise_file(path, &source, Level::Index).expect("summarise_file");
	let text = render_file_text(&file_summary);

	// Index should be a tiny fraction of the original.
	let ratio = compression_ratio(&source, &text);
	assert!(
		ratio < 15.0,
		"Index level should be <15% of original, was {ratio:.1}%"
	);
}

#[test]
fn signature_level_shrinks_rust_substantially() {
	let source = fixture("example.rs");
	let path = Path::new("example.rs");
	let file_summary = summarise_file(path, &source, Level::Signature).expect("summarise_file");
	let text = render_file_text(&file_summary);

	// Signature should be well under half the original.
	let ratio = compression_ratio(&source, &text);
	assert!(
		ratio < 50.0,
		"Signature level should be <50% of original, was {ratio:.1}%"
	);
}

#[test]
fn outline_level_is_smaller_than_original() {
	let source = fixture("example.rs");
	let path = Path::new("example.rs");
	let file_summary = summarise_file(path, &source, Level::Outline).expect("summarise_file");
	let text = render_file_text(&file_summary);

	// Even outline (most verbose) should be smaller than the source.
	let ratio = compression_ratio(&source, &text);
	assert!(
		ratio < 80.0,
		"Outline level should be <80% of original, was {ratio:.1}%"
	);
}

#[test]
fn index_is_most_compact() {
	let source = fixture("example.rs");
	let path = Path::new("example.rs");

	let index_text =
		render_file_text(&summarise_file(path, &source, Level::Index).expect("summarise_file"));
	let sig_text =
		render_file_text(&summarise_file(path, &source, Level::Signature).expect("summarise_file"));
	let outline_text =
		render_file_text(&summarise_file(path, &source, Level::Outline).expect("summarise_file"));

	assert!(
		index_text.len() <= sig_text.len(),
		"Index ({}) should be <= Signature ({})",
		index_text.len(),
		sig_text.len()
	);
	assert!(
		sig_text.len() <= outline_text.len(),
		"Signature ({}) should be <= Outline ({})",
		sig_text.len(),
		outline_text.len()
	);
}

#[test]
fn tree_sitter_languages_shrink_too() {
	// Verify tree-sitter languages also produce meaningful shrinkage.
	for (name, expected_max_ratio) in [
		("example.py", 25.0),
		("example.go", 25.0),
		("example.ts", 25.0),
		("example.java", 30.0),
	] {
		let source = fixture(name);
		let path = Path::new(name);
		let file_summary = summarise_file(path, &source, Level::Index).expect("summarise_file");
		let text = render_file_text(&file_summary);

		let ratio = compression_ratio(&source, &text);
		assert!(
			ratio < expected_max_ratio,
			"{name} at Index level should be <{expected_max_ratio}% of original, was {ratio:.1}%"
		);
	}
}
