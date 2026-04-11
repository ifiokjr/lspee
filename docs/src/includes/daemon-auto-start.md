Commands like `lspee status` and `lspee call` connect to an existing
daemon through the Unix socket at `<project_root>/.lspee/daemon.sock`.
If the connection fails (socket missing or refused), the CLI
transparently auto-starts a daemon and retries:

1. Attempt `UnixStream::connect()` on the socket path.
2. If successful, use the stream immediately.
3. If the socket does not exist or the connection is refused, call
   `spawn_daemon()` to start a new daemon process.
4. Retry the connection up to 40 times with 100 ms backoff (≈ 4 s
   total) while the daemon initializes and begins listening.
5. Return the connected stream once the daemon is ready.

This makes `lspee` zero-config for typical usage — you never need to
manually run `lspee serve` unless you want the daemon in the foreground
for debugging.

The `--no-start-daemon` flag disables auto-start for scripts that
should fail fast if no daemon is running. `lspee stop` always disables
auto-start since starting a daemon just to stop it is pointless.
