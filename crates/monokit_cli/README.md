# monokit_cli

[![codecov](https://codecov.io/gh/ifiokjr/monokit/branch/main/graph/badge.svg)](https://codecov.io/gh/ifiokjr/monokit)

[![Book](https://img.shields.io/badge/book-ifiokjr.github.io%2Fmonokit-blue)](https://ifiokjr.github.io/monokit/)

Command-line interface crate for `monokit`.

## Responsibility

`monokit_cli` owns:

- argument parsing,
- command/subcommand dispatch,
- conversion from CLI flags/options into calls into lower-level crates.

## What belongs here

- CLI UX and help text,
- top-level command routing (`serve`, `config`, etc.),
- process entrypoint (`main.rs`).

## What must NOT belong here

- daemon lifecycle implementation,
- LSP protocol or transport logic,
- core config model definitions.

## Allowed internal dependencies

- `monokit_daemon`
- `monokit_config`

## Notes

If logic starts looking reusable outside CLI argument handling, move it into the appropriate lower crate and keep this crate thin.

**Website:** <https://ifiokjr.github.io/monokit/>
