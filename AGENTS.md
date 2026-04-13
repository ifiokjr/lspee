# AGENTS.md

lspee is an agent-first LSP multiplexer for fast, shared, per-workspace language-server access. Agents are the primary users; human developers are secondary.

## Essentials

- Package manager: Cargo.
- Enter the dev environment with `devenv shell` before running repo commands.
- Preferred repo commands:
  - `build:all`
  - `test:all`
  - `lint:all`
  - `fix:all`
- Use `fix:format` / `dprint`; do not run `rustfmt` directly.
- Use `lint:clippy` (or `cargo clippy --workspace --all-targets -- -D warnings`) for clippy checks.
- All code changes go through a PR.

## Naming convention

- The project name is always written in **all lowercase**: `lspee`.
- Never use `Lspee`, `LSPEE`, or `LsPee` in prose, docs, comments, or string literals.
- **Rust code exception**: standard Rust naming conventions apply.
  - Structs and enums may use PascalCase (e.g. `LspeeConfig`).
  - Constants use UPPER_SNAKE_CASE (e.g. `LSPEE_VERSION`).
  - Variables and functions use snake_case (e.g. `lspee_config`).

## Git rules

- Never use `--no-verify` with `git commit` or `git push`.
- The only allowed exception is during `git rebase` workflows when a rebase continuation or amend step would otherwise block on hooks/editor behavior.
- Git hooks enforce formatting on commit (pre-commit) and full lint+test on push (pre-push).
- **All commits must use [Conventional Commits](https://www.conventionalcommits.org/) syntax**: `type(scope): description` (e.g., `feat(daemon): add memory eviction`, `fix(cli): resolve config parsing error`).

## Issue and PR rules

- **Issues**:
  - Use sentence case without a full stop at the end (e.g., "Add support for workspace symbols", "Fix memory leak in daemon").
  - Keep titles short enough to fit on one line.
  - Use backticks for code references when helpful (e.g., "`lspee do` should support multiple files").

- **Pull Requests**:
  - Must use Conventional Commits syntax for the PR title (e.g., `feat: add hover support`, `refactor: simplify session registry`).
  - PRs should use conventional commit syntax because they are squash-merged; the title becomes the commit message.
  - Provide a clear description of what changed and why.

## Quality rules

- **CRITICAL**: Never add `#[allow(clippy::...)]` attributes to functions or modules unless explicitly agreed upon with the maintainer. Fix clippy warnings instead.
- Edition 2024 Rust is used throughout the workspace.
- All clippy warnings must be resolved, not suppressed.

## Architecture

The workspace is organized into six crates under `crates/`:

- `lspee` - Reserved crate name (minimal API)
- `lspee_cli` - CLI binary (`lspee` command)
- `lspee_config` - Configuration loading, merging, and language registry
- `lspee_daemon` - Daemon process, session orchestration, eviction, memory management
- `lspee_lsp` - JSON-RPC/LSP process transport
- `lspee_protocol` - IPC wire models (control envelopes, stream frames)

## Available commands

All commands are available as devenv scripts. Run them inside `devenv shell` or prefix with `devenv shell --`:

| Command           | Description                             |
| ----------------- | --------------------------------------- |
| `build:all`       | Build all crates in the workspace       |
| `build:book`      | Build the mdbook documentation          |
| `test:all`        | Run all tests (nextest + doc tests)     |
| `test:cargo`      | Run cargo tests with nextest            |
| `test:docs`       | Run documentation tests                 |
| `lint:all`        | Run all checks (clippy + format + deny) |
| `lint:clippy`     | Check clippy lints                      |
| `lint:format`     | Check dprint formatting                 |
| `fix:all`         | Fix all autofixable problems            |
| `fix:clippy`      | Auto-fix clippy lints                   |
| `fix:format`      | Format files with dprint                |
| `deny:check`      | Run cargo-deny security/license checks  |
| `coverage:all`    | Generate lcov coverage report           |
| `install:all`     | Install all required cargo binaries     |
| `snapshot:review` | Review insta snapshots                  |
| `snapshot:update` | Update insta snapshots                  |

## NPM package publishing

The `@ifi/lspee` npm package provides cross-platform binary distribution:

- Platform packages: `@ifi/lspee-darwin-arm64`, `@ifi/lspee-darwin-x64`, `@ifi/lspee-linux-arm64-gnu`, etc.
- Root package: `@ifi/lspee` (auto-selects correct platform binary)
- Skill package: `@ifi/lspee-skill`

Publishing is automated via the `release` and `npm-publish` GitHub workflows.
