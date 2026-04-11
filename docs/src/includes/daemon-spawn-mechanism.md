When the CLI needs a daemon and one is not already running, it spawns a
new `lspee serve` process as a detached background child:

```text
std::process::Command::new(current_exe)
    .arg("serve")
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
```

Three properties make this work without any explicit daemonization
(no `fork`, `setsid`, or `nohup`):

1. **stdio is nullified** — the child has no connection to the
   terminal, so closing the terminal does not deliver `SIGHUP` through
   a controlling tty.
2. **the handle is dropped** — the CLI does not store or wait on the
   child process handle, so the child immediately becomes independent.
3. **orphan reparenting** — when the CLI process exits, the OS
   reparents the now-orphaned daemon to `init` (PID 1 on Linux) or
   `launchd` (PID 1 on macOS). The daemon continues to run
   indefinitely under the system's top-level process supervisor.

Logs are written to `<project_root>/.lspee/daemon.log` so they remain
accessible even though stderr is disconnected. The `LSPEE_LOG` and
`LSPEE_LOG_FORMAT` environment variables are forwarded to the child so
logging configuration set by the caller is preserved.
