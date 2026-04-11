# lspee_cli

[![Book](https://img.shields.io/badge/book-ifiokjr.github.io%2Flspee-blue)](https://ifiokjr.github.io/lspee/)

Command-line interface crate for `lspee`.

## Responsibility

`lspee_cli` owns:

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

- `lspee_daemon`
- `lspee_config`

## Notes

If logic starts looking reusable outside CLI argument handling, move it into the appropriate lower crate and keep this crate thin.

**Website:** <https://ifiokjr.github.io/lspee/>
