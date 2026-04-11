### macOS

On macOS, orphaned processes are reparented to `launchd` (PID 1),
which acts as both the init system and the service manager. The
daemon process persists across terminal closures, SSH disconnects,
and user logouts (unless the user's login session is configured to
kill background processes). This is the same mechanism that tools
like `nohup` rely on, but lspee achieves it without `nohup` by
nullifying stdio before spawning.

The control socket at `.lspee/daemon.sock` uses macOS's native Unix
domain socket support (`AF_UNIX`), which is fully supported by the
Darwin kernel and works identically to Linux.

### Linux

On Linux, orphaned processes are reparented to the init process
(PID 1), which is typically `systemd`, `openrc`, or a similar init
system. The behavior is the same as macOS: the daemon survives
terminal closure because it has no controlling terminal attached.

If the system uses `systemd` with `KillUserProcesses=yes` in
`logind.conf`, user processes may be killed on logout. Users in this
configuration should either disable that setting or run lspee inside
a `systemd-run --scope` wrapper for persistent sessions.

### Windows (future)

Windows does not have Unix domain sockets or the same orphan
reparenting model. Future Windows support will require:

- **Named pipes** (`\\.\pipe\lspee-<root-hash>`) as the IPC
  transport instead of Unix sockets. The tokio ecosystem supports
  named pipes via `tokio::net::windows::named_pipe`.
- **Process detachment** — on Windows, the `CREATE_NO_WINDOW` and
  `DETACHED_PROCESS` creation flags serve a similar role to
  stdio nullification on Unix. The daemon would be spawned with
  these flags to prevent it from being attached to the console
  window.
- **Discovery paths** — socket path conventions differ; the daemon
  would use a well-known pipe name derived from the project root
  hash rather than a filesystem path.
- **CI coverage** — tests would need to run on `windows-latest` in
  GitHub Actions to validate pipe transport and process lifecycle.

The core architecture (session registry, NDJSON protocol, eviction
policy) is platform-independent. Only the transport layer and
process-spawning code are Unix-specific, which limits the scope of
a Windows port.
