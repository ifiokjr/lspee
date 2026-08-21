# monokit_lsp

[![codecov](https://codecov.io/gh/ifiokjr/monokit/branch/main/graph/badge.svg)](https://codecov.io/gh/ifiokjr/monokit)

[![Book](https://img.shields.io/badge/book-ifiokjr.github.io%2Fmonokit-blue)](https://ifiokjr.github.io/monokit/)

LSP transport/protocol integration crate for `monokit`.

## Responsibility

`monokit_lsp` owns Language Server Protocol facing concerns:

- transport setup,
- request/notification wiring,
- protocol-level preparation and glue.

## What belongs here

- JSON-RPC/LSP integration code,
- server capability wiring,
- protocol adapter logic between runtime and LSP events.

## What must NOT belong here

- CLI argument parsing,
- daemon process lifecycle policy,
- primary config ownership.

## Allowed internal dependencies

- `monokit_config`

## Notes

Keep protocol concerns encapsulated here so daemon and CLI crates can remain focused on orchestration and UX respectively.

**Website:** <https://ifiokjr.github.io/monokit/>
