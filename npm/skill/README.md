# @ifi/monokit-skill

Agent skill package for [monokit](https://github.com/ifiokjr/monokit) — teaches AI agents how to use LSP servers for code intelligence.

## Install

```bash
npm install -g @ifi/monokit-skill
```

## Usage

```bash
# Print the concise agent skill guide
monokit-skill --print-skill

# Print the full command reference
monokit-skill --print-reference

# Copy skill files to a directory
monokit-skill --copy ~/.config/agent-skills/monokit

# Print installation instructions
monokit-skill --print-install
```

## What's included

- **SKILL.md** — Concise instructions for AI agents (quick start, working rules, common workflow)
- **REFERENCE.md** — Complete CLI reference, JSON-RPC request formats, configuration, error codes

## Related

- [@ifi/monokit](https://www.npmjs.com/package/@ifi/monokit) — the CLI binary
- [monokit on GitHub](https://github.com/ifiokjr/monokit)
