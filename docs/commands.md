# CLI Commands Reference

## `mso run` — Run a supervised process

Registers a command with the background daemon. The daemon spawns the process, captures its stdout/stderr, and monitors its lifecycle.

```bash
mso run [OPTIONS] <COMMAND...>
```

### Arguments

| Argument | Description |
|----------|-------------|
| `COMMAND` | Command and arguments to execute (trailing var arg, all remaining args are consumed) |

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `-s`, `--silence [<SECS>]` | Seconds to stream output before backgrounding. `-s 0` backgrounds immediately. `-s` with no value = `-s 0` | Stream forever (no auto-background) |
| `--restart <POLICY>` | Auto-restart policy: `"no"` or `"always"` | `"no"` (or config file value) |
| `--tag <TAG>` | Tag for filtering in the TUI. Can be specified multiple times | — |
| `--health-check <URL>` | Health check URL. Daemon HTTP GETs this endpoint periodically | — |
| `--health-interval <SECS>` | Seconds between health checks | `10` |
| `--health-timeout <SECS>` | HTTP request timeout | `5` |
| `--health-max-failures <N>` | Consecutive failures before restart | `3` |
| `--alert-webhook <URL>` | Webhook URL for exit/crash notifications. Sends JSON POST | — |
| `--preset <NAME>` | Load preset configuration from `~/.mso/presets/` | — |
| `--depends-on <UUID>` | Wait for dependency's health check before starting (can repeat) | — |

### Examples

```bash
# Stream for 5 seconds, then background
mso run -s 5 python3 -m http.server 8080

# Background immediately with auto-restart and tags
mso run --restart always --tag web --tag prod -s 0 node server.js

# With health checks and alerting
mso run --restart always \
  --health-check http://localhost:8080/health \
  --alert-webhook https://hooks.slack.com/services/... \
  -s 0 \
  python3 app.py

# With process dependency
mso run --restart always -s 0 --depends-on <db-uuid> my-app

# With Docker
```

---

## `mso view` — Open the TUI dashboard

The default command when no subcommand is given. Opens the interactive terminal dashboard showing all managed processes.

```bash
mso view
# or just:
mso
```

See [tui.md](tui.md) for full key bindings and features.

---

## `mso exec` — Run a command directly

Runs a command with inherited stdio and exits with the child's exit code. No daemon is spawned. Useful for CI/CD or one-off commands where you don't need supervision.

```bash
mso exec <COMMAND...>
```

### Example

```bash
mso exec cargo test
# Runs cargo test, inherits stdout/stderr, exits with cargo's exit code
```

---

## `mso config` — Manage configuration

Validates, shows, or locates the MSO configuration file.

```bash
mso config <ACTION>
```

### Actions

| Action | Description |
|--------|-------------|
| `validate` | Check `~/.mso/config.toml` for errors |
| `show` | Print the current configuration |
| `path` | Show the config file path |

### Examples

```bash
mso config validate
# [mso] ✓ configuration is valid
mso config show
mso config path
# /home/user/.mso/config.toml
```

---

## `mso preset` — Manage process presets

Save, list, and remove process presets. Stored as TOML files in `~/.mso/presets/`.

```bash
mso preset <ACTION> [OPTIONS]
```

### Actions

| Action | Description |
|--------|-------------|
| `list` | List all saved presets |
| `save --name <NAME> -- <COMMAND>` | Save a new preset |
| `remove --name <NAME>` | Remove a preset |

### Example

```bash
mso preset save --name web-server -- python3 -m http.server 8080
mso run --preset web-server
mso preset list
```

---

## `mso stats` — Show process statistics

Prints a table of all managed processes to stdout.

```bash
mso stats [OPTIONS]
```

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `--format <FORMAT>` | Output format: `text` or `json` | `text` |

### Example

```bash
mso stats
# PID      NAME        CPU%   MEM        PORTS     STATUS
# 277760   bash        23%    42.5MB     :8080     Running

mso stats --format json
# [{"pid":277760,"command":["bash"],...}]
```

---

## `mso export` / `mso import` — Transfer process configurations

Export process registrations as JSON for backup or transfer to another machine.

```bash
mso export [OPTIONS]
mso import <FILE>
```

### Options

| Option | Description |
|--------|-------------|
| `-o`, `--output <FILE>` | Write to file instead of stdout |
| `--process <UUID>` | Export only a specific process |

### Example

```bash
mso export --output my-processes.json
mso import my-processes.json
```

---

## `mso self-update` — Update MSO

Checks GitHub for the latest release and updates the binary.

```bash
mso self-update
```

Requires a published GitHub release with binaries for your platform.

---

## `mso logs` — Export process logs

Connects to the daemon and exports log lines for a specific process. Supports partial UUID matching (first 8 characters) or PID lookup.

```bash
mso logs <PID_OR_UUID> [OPTIONS]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `PID_OR_UUID` | Process PID number or UUID (partial match supported) |

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `--format <FORMAT>` | Output format: `"text"` or `"json"` | `"text"` |
| `--tail <N>` | Only show the last N lines | All lines (up to 10000) |

### Examples

```bash
# Show last 50 lines as text
mso logs 277760 --tail 50

# Show all logs as JSON (partial UUID match)
mso logs abc12345 --format json

# Pipe to grep
mso logs 277760 | grep ERROR
```

---

## `mso prune` — Prune old logs

Deletes log entries older than a specified number of days. Can optionally target a specific process.

```bash
mso prune [OPTIONS]
```

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `--days <N>` | Delete logs older than N days | `30` |
| `--process <UUID>` | Only prune logs for a specific process UUID | All processes |

### Examples

```bash
# Prune logs older than 7 days
mso prune --days 7

# Prune logs for a specific process
mso prune --days 30 --process abc12345-...
```

The daemon also runs an automatic prune every hour, deleting logs older than 30 days.

---

## `mso completion` — Generate shell completions

Generates shell completion scripts for the `mso` command.

```bash
mso completion <SHELL>
```

### Arguments

| Argument | Supported Values |
|----------|-----------------|
| `SHELL` | `bash`, `zsh`, `fish`, `powershell`, `elvish` |

### Examples

```bash
# Install bash completions
mso completion bash > /etc/bash_completion.d/mso

# Install zsh completions
mso completion zsh > /usr/local/share/zsh/site-functions/_mso
```

---

## `mso systemd` — Systemd service management

Manages a systemd user service for automatic daemon startup on login.

```bash
mso systemd <ACTION>
```

### Actions

| Action | Description |
|--------|-------------|
| `install` | Generate `~/.config/systemd/user/mso.service`, run `systemctl --user daemon-reload`, enable and start |
| `uninstall` | Stop, disable, and remove the systemd service |
| `status` | Check if the service is running |

### Examples

```bash
# Install and start the service
mso systemd install

# Check status
mso systemd status

# Uninstall
mso systemd uninstall
```

---

## `mso connect` — Remote daemon via SSH

Establishes an SSH tunnel to a remote MSO daemon and opens the TUI connected to it.

```bash
mso connect <HOST> [OPTIONS]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `HOST` | SSH destination in `user@host` format |

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `--socket <PATH>` | Local socket path for the forwarded tunnel | `/tmp/mso-remote.sock` |

### How it works

1. Spawns `ssh -N -L /tmp/mso-remote.sock:<remote>:~/.mso/mso.sock user@host`
2. Waits for the local socket to appear (polls for up to 10 seconds)
3. Opens the TUI dashboard connected to the forwarded socket
4. On exit, kills the SSH tunnel and removes the local socket

### Example

```bash
mso connect deploy@server.example.com
```

---

## `mso daemon` — Start the daemon (internal)

Starts the background daemon process. This is normally started automatically by `mso run` or `mso view` — you should not need to run it manually.

```bash
mso daemon
```
