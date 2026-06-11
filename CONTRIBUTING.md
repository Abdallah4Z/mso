# Contributing to MSO

## Setup

```bash
git clone <repo>
cd mso
cargo build
```

## Development

```bash
# Run the project
cargo run -- <command>

# Run tests (unit + integration + snapshot)
cargo test

# Run benchmarks
cargo bench

# Check for issues
cargo clippy
```

## Code Style

- 0 clippy warnings before submitting PRs
- All functions should be under 50 lines — extract helpers for longer functions
- Prefix unused variables with `_` (e.g. `_event_tx`)
- Use `anyhow::Result` for fallible functions
- Use `tracing::info!`/`warn!`/`error!` for logging (not `eprintln!`)
- Handle all errors — avoid `unwrap()` in production code, prefer `?` or `if let Err(e) = ... { tracing::warn!(...) }`

## Testing

| Test Type | Command | Location |
|-----------|---------|----------|
| Unit | `cargo test --lib` | `src/**/*.rs` `#[cfg(test)]` |
| Integration | `cargo test --test integration` | `tests/integration.rs` |
| Snapshot | `cargo test --test tui_snapshot` | `tests/tui_snapshot.rs` |
| All | `cargo test` | — |

When adding new features:
1. Add unit tests for core logic
2. Add integration tests for daemon protocol
3. Add snapshot tests for TUI widgets

## Project Structure

```
src/
├── main.rs              # CLI entry (thin dispatch)
├── lib.rs               # Library re-exports
├── commands.rs          # All command handler functions
├── cli.rs               # Clap argument definitions
├── protocol.rs          # Wire format + shared types
├── util.rs              # Config, paths, presets
├── client/              # Client-side (runner, daemonize, remote)
├── daemon/              # Server-side (supervisor, listener, telemetry...)
└── tui/                 # Terminal UI (app, widgets, theme)
```

## Commit Messages

Follow conventional commits: `feat:`, `fix:`, `docs:`, `perf:`, `test:`, `refactor:`, `chore:`

## PR Checklist

- [ ] `cargo clippy` passes with 0 warnings
- [ ] `cargo test` passes (all 25+ tests)
- [ ] `cargo build --release` succeeds
- [ ] New features have tests
- [ ] Docs updated (README.md or docs/)
