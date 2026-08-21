# Release Process

## Version policy

`monokit` follows SemVer:

- MAJOR: breaking CLI/protocol/config changes
- MINOR: backward-compatible features
- PATCH: fixes and non-breaking improvements

## Pre-release checklist

```bash
cargo fmt --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```

## Documentation checklist

- update `README.md`
- update book pages under `docs/src`
- update `CHANGELOG.md`
- add `.changeset/*.md` entries for included updates

## Packaging notes

- CLI package: `monokit_cli` (installs `monokit` binary)
- reservation package: `monokit`

## CI

The default CI workflow runs formatting, check, clippy, and tests on push/PR.
