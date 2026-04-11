# lspee_protocol

[![Book](https://img.shields.io/badge/book-ifiokjr.github.io%2Flspee-blue)](https://ifiokjr.github.io/lspee/)

Shared control-protocol models for `lspee`.

## Responsibility

`lspee_protocol` owns the canonical type definitions for the daemon control protocol:

- request/response envelope types,
- error codes and constants,
- shared serialization models.

## What belongs here

- Protocol structs/enums used by both daemon and CLI,
- error code constants,
- NDJSON envelope definitions.

## What must NOT belong here

- daemon runtime logic,
- CLI argument parsing,
- LSP transport machinery,
- configuration ownership.

## Dependency posture

- Should not depend on other internal `lspee_*` crates.
- Should stay minimal and stable.

**Website:** <https://ifiokjr.github.io/lspee/>
