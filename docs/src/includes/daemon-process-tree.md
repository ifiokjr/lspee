```text
Terminal session
 └─ lspee call ...              CLI process (short-lived)
     └─ lspee serve             daemon process (long-lived, stdio=null)
         ├─ rust-analyzer       LSP child process (managed by daemon)
         ├─ gopls               LSP child process (managed by daemon)
         └─ ...
```

The CLI exits as soon as it receives a response. The daemon and its LSP children survive because they are fully detached from the terminal. On the next CLI invocation, the CLI reconnects to the existing daemon through the Unix socket — no new daemon or LSP processes are spawned if the session key matches.

When the daemon is eventually stopped (via `lspee stop` or system shutdown), it sends LSP `shutdown`/`exit` to each child, waits up to 2 seconds for graceful termination, then force-kills any remaining processes and removes the socket file.
