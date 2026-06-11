use std::path::PathBuf;

/// ~/.mso/
pub fn mso_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mso")
}

/// ~/.mso/mso.sock
pub fn mso_sock_path() -> PathBuf {
    mso_dir().join("mso.sock")
}

/// ~/.mso/daemon.pid
pub fn mso_pid_path() -> PathBuf {
    mso_dir().join("daemon.pid")
}

/// ~/.mso/config.toml
pub fn mso_config_path() -> PathBuf {
    mso_dir().join("config.toml")
}

/// Ensure ~/.mso/ exists.
pub fn ensure_mso_dir() -> std::io::Result<()> {
    let dir = mso_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(())
}

/// Current epoch millis.
pub fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Read the daemon PID from the pid file, if it exists and is alive.
pub fn read_daemon_pid() -> Option<u32> {
    let pid_file = mso_pid_path();
    let content = std::fs::read_to_string(&pid_file).ok()?;
    let pid: u32 = content.trim().parse().ok()?;
    let ret = unsafe { libc::kill(pid as i32, 0) };
    if ret == 0 {
        Some(pid)
    } else {
        let _ = std::fs::remove_file(&pid_file);
        None
    }
}

/// Write the daemon PID to the pid file.
pub fn write_daemon_pid(pid: u32) -> std::io::Result<()> {
    ensure_mso_dir()?;
    std::fs::write(mso_pid_path(), pid.to_string())
}

// ── Configuration ──────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Config {
    #[serde(default = "default_restart")]
    pub restart_policy: String,
    #[serde(default)]
    pub silence_secs: Option<u64>,
    #[serde(default = "default_log_retention")]
    pub log_retention_days: u64,
    #[serde(default)]
    pub theme: Option<ThemeConfig>,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct ThemeConfig {
    pub accent: Option<String>,
    pub bg_dark: Option<String>,
    pub bg_mid: Option<String>,
}

fn default_restart() -> String { "no".into() }
fn default_log_retention() -> u64 { 30 }

impl Default for Config {
    fn default() -> Self {
        Self {
            restart_policy: "no".into(),
            silence_secs: None,
            log_retention_days: 30,
            theme: None,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = mso_config_path();
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&content).unwrap_or_default()
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if !["no", "always"].contains(&self.restart_policy.as_str()) {
            errors.push("restart_policy must be 'no' or 'always'".into());
        }
        if let Some(secs) = self.silence_secs {
            if secs > 86400 {
                errors.push("silence_secs must be ≤ 86400".into());
            }
        }
        if self.log_retention_days == 0 {
            errors.push("log_retention_days must be > 0".into());
        }
        if let Some(ref theme) = self.theme {
            for (name, val) in [("accent", &theme.accent), ("bg_dark", &theme.bg_dark), ("bg_mid", &theme.bg_mid)] {
                if let Some(ref v) = val {
                    if !v.starts_with('#') || v.len() != 7 || v[1..].chars().any(|c| !c.is_ascii_hexdigit()) {
                        errors.push(format!("theme.{name} must be a valid hex color like #00CCFF"));
                    }
                }
            }
        }
        errors
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Preset {
    pub command: Vec<String>,
    #[serde(default)]
    pub restart_policy: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub silence_secs: Option<u64>,
    pub health_check: Option<String>,
    pub health_interval: Option<u64>,
    pub health_timeout: Option<u64>,
    pub health_max_failures: Option<u32>,
}

pub fn load_preset(name: &str) -> Option<Preset> {
    let path = mso_dir().join("presets").join(format!("{name}.toml"));
    let content = std::fs::read_to_string(&path).ok()?;
    toml::from_str(&content).ok()
}
