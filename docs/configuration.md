# Configuration

MSO reads configuration from `~/.mso/config.toml` if the file exists. All values are optional — defaults are used for any missing fields.

## File Location

```
~/.mso/config.toml
```

## Format

```toml
# Global defaults
restart_policy = "always"       # Default restart policy: "no" or "always"
silence_secs = 5                # Default silence seconds for `mso run`
log_retention_days = 90         # Auto-prune logs older than this (daemon-side)

[theme]                         # TUI color overrides
accent = "#00CCFF"              # Accent color (hex RGB)
bg_dark = "#0A0A10"             # Dark background
bg_mid = "#10121C"              # Medium background
```

## Fields

### `restart_policy` (string, default: `"no"`)

Default restart policy for `mso run`. CLI `--restart` flag overrides this.

```toml
restart_policy = "always"
```

Values: `"no"` | `"always"`

### `silence_secs` (integer, optional)

Default silence duration for `mso run`. If not set and no `-s` flag is given, output streams forever (no auto-background).

```toml
silence_secs = 10
```

### `log_retention_days` (integer, default: `30`)

Logs older than this many days are automatically pruned every hour by the daemon.

```toml
log_retention_days = 90
```

### `[theme]` section

Override TUI colors. These values are now applied (previously ignored):

```toml
[theme]
accent = "#00CCFF"
bg_dark = "#0A0A10"
bg_mid = "#10121C"
```

Available keys:

| Key | Default | Description |
|-----|---------|-------------|
| `accent` | `#00C8FF` | Primary accent (borders, highlights, CPU bars) |
| `bg_dark` | `#0A0A10` | Main background |
| `bg_mid` | `#10121C` | Panel backgrounds |

Available theme keys:

| Key | Default | Description |
|-----|---------|-------------|
| `accent` | `#00C8FF` | Primary accent (borders, highlights) |
| `bg_dark` | `#0A0A10` | Main background |
| `bg_mid` | `#10121C` | Panel backgrounds |

## Precedence

CLI flags > Config file > Built-in defaults

```
mso run --restart always          # Uses --restart flag (highest priority)
mso run                           # Uses config file value (or built-in default)
```

## Example Config

```toml
restart_policy = "always"
silence_secs = 5
log_retention_days = 90

[theme]
accent = "#00FFCC"
bg_dark = "#050510"
```

This config would:
- Default all `mso run` commands to auto-restart on crash
- Stream output for 5 seconds before backgrounding
- Prune logs older than 90 days
- Use a teal accent on a slightly darker background
