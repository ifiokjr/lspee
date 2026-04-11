The daemon automatically shuts itself down when it has had zero sessions
for a configurable period. This prevents orphaned daemon processes from
accumulating on your system.

```toml
[session]
daemon_idle_ttl_secs = 1800  # default: 30 minutes
```

The lifecycle works as follows:

1. The daemon starts (auto-started by CLI or manually via `lspee serve`).
2. Clients attach sessions; the daemon keeps LSP processes warm.
3. Sessions are evicted individually after `idle_ttl_secs` with no
   active leases (default: 300s / 5 minutes).
4. Once the last session is evicted and the registry is empty, the
   daemon starts a countdown of `daemon_idle_ttl_secs`.
5. If a new client attaches before the countdown expires, the timer
   resets.
6. If the countdown reaches zero, the daemon performs a clean shutdown:
   removes the socket file and exits.

To disable auto-shutdown entirely (daemon runs until explicitly stopped
or the machine reboots), set:

```toml
[session]
daemon_idle_ttl_secs = 0
```

When `daemon_idle_ttl_secs` is `0`, the daemon only stops via
`lspee stop`, a `Shutdown` control message, or a system signal.
