# @ifi/lspee-skill

Agent skill package for [lspee](https://github.com/ifiokjr/lspee) — teaches AI agents how to use LSP servers for code intelligence.

## Install

```bash
npm install -g @ifi/lspee-skill
```

## Usage

```bash
# Print the concise agent skill guide
lspee-skill --print-skill

# Print the full command reference
lspee-skill --print-reference

# Copy skill files to a directory
lspee-skill --copy ~/.config/agent-skills/lspee

# Print installation instructions
lspee-skill --print-install
```

## What's included

- **SKILL.md** — Concise instructions for AI agents (quick start, working rules, common workflow)
- **REFERENCE.md** — Complete CLI reference, JSON-RPC request formats, configuration, error codes

## Related

- [@ifi/lspee](https://www.npmjs.com/package/@ifi/lspee) — the CLI binary
- [lspee on GitHub](https://github.com/ifiokjr/lspee)
