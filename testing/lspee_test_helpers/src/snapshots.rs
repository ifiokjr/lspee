use insta::Settings;

/// Returns insta snapshot settings with standard redactions for
/// dynamic values that change across test runs.
pub fn snapshot_settings() -> Settings {
	let mut settings = Settings::clone_current();

	// Redact temporary directory paths
	settings.add_filter(r"/tmp/[a-zA-Z0-9._-]+", "[TEMP_DIR]");
	settings.add_filter(r"/private/tmp/[a-zA-Z0-9._-]+", "[TEMP_DIR]");

	// Redact config hashes (SHA256 hex)
	settings.add_filter(r"[0-9a-f]{64}", "[CONFIG_HASH]");

	// Redact timestamps (nanosecond IDs)
	settings.add_filter(r"\d{18,}", "[TIMESTAMP]");

	// Redact lease IDs
	settings.add_filter(r"lease_\d+", "lease_[N]");

	settings
}
