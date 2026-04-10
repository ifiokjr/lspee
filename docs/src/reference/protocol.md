# Daemon Protocol Reference

Control channel uses NDJSON frames over a local Unix socket.

Envelope:

```json
{
  "v": 1,
  "id": "req-1",
  "type": "Attach",
  "payload": {}
}
```

## Control request/response pairs

- `Attach` / `AttachOk`
- `Call` / `CallOk`
- `Release` / `ReleaseOk`
- `Stats` / `StatsOk`
- `Shutdown` / `ShutdownOk`
- `Error`

## Attach modes

`Attach.capabilities.stream_mode` supports:

- `mux_control` — control-only clients (`lspee call`)
- `dedicated` — separate per-lease stream socket (`lspee proxy`)

When `dedicated` is requested, `AttachOk.stream.endpoint` returns a Unix socket endpoint for `StreamFrame` traffic.

## Dedicated stream frames

- `LspIn` — client/editor to backend runtime
- `LspOut` — backend runtime to client/editor
- `StreamError` — terminal warning/error, including memory-budget eviction notices

## Canonical definitions

See `crates/lspee_protocol/src/lib.rs` for authoritative structs/constants.

## Important error codes

- `E_BAD_MESSAGE`
- `E_UNSUPPORTED_VERSION`
- `E_INVALID_SESSION_KEY`
- `E_SESSION_SPAWN_FAILED`
- `E_LEASE_NOT_FOUND`
- `E_SESSION_EVICTED_MEMORY`
- `E_INTERNAL`
