# Installation

## Prerequisites

- Rust stable toolchain (1.85+ recommended)
- A Unix-like OS (Linux/macOS) for v0.1.x
- LSP binaries you plan to use (`rust-analyzer`, `gopls`, etc.)

## Build from source

```bash
git clone <your-repo-url>
cd lspee
cargo build --release -p lspee_cli
```

Binary path:

```text
target/release/lspee
```

## Install CLI locally

```bash
cargo install --path crates/lspee_cli
```

## Verify

```bash
lspee --help
lspee status --output json
```

`status` auto-starts daemon by default when missing.

## Optional: run checks

```bash
cargo fmt --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
