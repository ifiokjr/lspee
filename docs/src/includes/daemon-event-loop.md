The daemon runs on a tokio async runtime with a central event loop
that multiplexes two concerns:

```text
loop {
    select! {
        stream = listener.accept() => handle_client(stream),
        signal = shutdown_rx.changed() => break,
    }
}
```

- **Client connections** — each accepted Unix stream is spawned into
  its own async task via `tokio::spawn()`. This means the daemon can
  serve many concurrent CLI invocations and editor proxies without
  blocking. Each task reads NDJSON frames, dispatches control
  requests (`Attach`, `Call`, `Release`, `Stats`, `Shutdown`), and
  writes responses back on the same stream.
- **Shutdown signal** — a `watch` channel carries the shutdown flag.
  When any client sends a `Shutdown` request, the sender fires the
  channel and the loop breaks. The daemon then gracefully shuts down
  all LSP sessions (sending `shutdown` + `exit` to each child
  process), removes the socket file, and exits.

Between connections, the daemon keeps LSP child processes warm in
the session registry. Background tasks handle idle eviction
(`EvictionLoop`) and memory monitoring (`MemoryMonitor`), both of
which run on their own tokio-spawned intervals.
