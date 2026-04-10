# Platform Support

## Current support (v0.1.x)

- ✅ Linux
- ✅ macOS
- ❌ Windows (named pipe transport not implemented yet)

`lspee` currently uses Unix domain sockets and emits a clear compile-time error on unsupported platforms.

## Why

Daemon and CLI control transport are implemented with `tokio::net::UnixListener/UnixStream`.

## Forward plan

Windows support will require:

- named pipe transport
- platform-conditional daemon discovery paths
- CI coverage on windows-latest
