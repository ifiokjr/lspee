# monokit_config

[![codecov](https://codecov.io/gh/ifiokjr/monokit/branch/main/graph/badge.svg)](https://codecov.io/gh/ifiokjr/monokit)

[![Book](https://img.shields.io/badge/book-ifiokjr.github.io%2Fmonokit-blue)](https://ifiokjr.github.io/monokit/)

Configuration model crate for `monokit`.

## Responsibility

`monokit_config` provides configuration domain types and resolution helpers.

## What belongs here

- resolved configuration structs/enums,
- parsing/normalization helpers (as needed),
- defaults and config-merging behavior.

## What must NOT belong here

- daemon runtime lifecycle code,
- command-line UX concerns,
- LSP transport/protocol machinery.

## Dependency posture

- Should not depend on other internal `monokit_*` crates.
- Should stay broadly reusable and low-level.

## Notes

Keep this crate deterministic and side-effect-light where practical; it is the safest place for shared, stable domain configuration types.

The built-in `defaults/languages.toml` catalog is Helix-inspired and currently seeds 100 LSP entries.

**Website:** <https://ifiokjr.github.io/monokit/>
