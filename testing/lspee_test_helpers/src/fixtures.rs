use std::path::{Path, PathBuf};

/// Returns the path to the project-root `fixtures/` directory.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
}

/// Returns the path to a named fixture directory.
pub fn fixture_path(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

/// Copies a fixture directory to a temporary directory and returns the path.
/// The temp directory is cleaned up when the returned `TempDir` is dropped.
pub fn setup_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    let source = fixture_path(name);
    assert!(
        source.exists(),
        "fixture '{}' not found at {}",
        name,
        source.display()
    );

    let temp = tempfile::tempdir().expect("should create temp dir");
    copy_dir_recursive(&source, temp.path()).expect("should copy fixture");
    let canonical = std::fs::canonicalize(temp.path()).expect("should canonicalize");
    (temp, canonical)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
