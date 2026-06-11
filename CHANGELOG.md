# Changelog

## v0.1.0 (2026-06-11)

### Added
- Initial release
- Daemon-client architecture with Unix domain socket communication
- `mso run` — run supervised processes with auto-restart, health checks, tagging, dependencies
- `mso view` — interactive TUI dashboard with neon/dark/light themes
- `mso exec` — run commands directly (no daemon)
- `mso logs` — export process logs as text or JSON
- `mso prune` — delete old log entries
- `mso stats` — print process statistics to terminal
- `mso completion` — generate shell completions (bash, zsh, fish, powershell, elvish)
- `mso config` — validate, show, or locate configuration
- `mso preset` — save, list, and remove process presets
- `mso systemd` — install/uninstall/status systemd user service
- `mso connect` — SSH tunnel to remote daemon
- SQLite log persistence with search and pagination
- Prometheus metrics endpoint on port 9753
- Webhook alerts on process exit/crash
- Process dependencies (`--depends-on`)
- Process tagging with TUI tag filter
- Auto-follow log mode (F3 toggle)
- ANSI color passthrough in log viewer
- Notification center (N key)
- Read-only mode (`mso view --readonly`)
- Process list search (f key)
- Process reordering (Ctrl+Up/Down)
- Resizable sidebar panels
- Mouse support (click, scroll, drag)
- 25+ tests (unit, integration, snapshot)
- 6 performance benchmarks

### Fixed
- CPU telemetry data race (shared static mut → per-PID HashMap)
- Zombie process on auto-restart (child.wait() before drop)
- --alert-webhook flag not being passed to daemon
- Theme configuration from config.toml not being applied
- Log view hardcoded accent color
- Poisoned mutex recovery in LogDb
- TOCTOU race in exit monitor DashMap access
- unreachable!() panic in listener (→ warning + continue)
- Dead code: ring_buffer, decode_message, parse_length_prefix, block_child_signals
- 4 clippy warnings eliminated (complex type, redundant guards, strip_prefix)
- Misleading field name: network_bytes → io_bytes
