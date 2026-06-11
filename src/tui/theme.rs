use ratatui::prelude::Color;
use ratatui::layout::Rect;
use std::sync::OnceLock;

/// Active theme colors — set once on startup
static THEME: OnceLock<ThemeColors> = OnceLock::new();

pub fn init(theme: Theme) {
    let _ = THEME.set(theme.colors());
}

pub fn init_with_colors(colors: ThemeColors) {
    let _ = THEME.set(colors);
}

pub fn colors() -> &'static ThemeColors {
    THEME.get_or_init(|| Theme::Neon.colors())
}

/// Available theme variants
#[derive(Clone, Copy, Debug, Default)]
pub enum Theme {
    #[default]
    Neon,
    #[allow(dead_code)]
    Dark,
    #[allow(dead_code)]
    Light,
}

impl Theme {
    pub fn from_config(_cfg: &Option<crate::util::ThemeConfig>) -> Self {
        // Theme selection could be extended; default to Neon with optional overrides
        Self::Neon
    }

    pub fn colors_with_overrides(&self, cfg: &Option<crate::util::ThemeConfig>) -> ThemeColors {
        let mut c = self.colors();
        if let Some(ref theme) = cfg {
            if let Some(ref accent) = theme.accent {
                if let Ok(color) = parse_hex_color(accent) {
                    c.accent = color;
                    c.bar_cpu = color;
                }
            }
            if let Some(ref bg) = theme.bg_dark {
                if let Ok(color) = parse_hex_color(bg) {
                    c.bg_dark = color;
                }
            }
            if let Some(ref bg) = theme.bg_mid {
                if let Ok(color) = parse_hex_color(bg) {
                    c.bg_mid = color;
                }
            }
        }
        c
    }

    pub fn colors(&self) -> ThemeColors {
        match self {
            Theme::Neon => ThemeColors {
                bg_dark: Color::Rgb(10, 10, 16),
                bg_mid: Color::Rgb(16, 18, 28),
                bg_light: Color::Rgb(22, 25, 38),
                border: Color::Rgb(35, 40, 55),
                accent: Color::Rgb(0, 180, 220),
                accent_green: Color::Rgb(0, 230, 120),
                accent_red: Color::Rgb(230, 50, 70),
                accent_yellow: Color::Rgb(255, 200, 50),
                accent_purple: Color::Rgb(160, 80, 255),
                text_bright: Color::Rgb(200, 210, 220),
                text_dim: Color::Rgb(80, 85, 100),
                text_faded: Color::Rgb(50, 55, 70),
                bar_cpu: Color::Rgb(0, 180, 220),
                bar_mem: Color::Rgb(255, 180, 0),
                bar_io: Color::Rgb(160, 80, 255),
                row_selected: Color::Rgb(25, 32, 60),
                row_hover: Color::Rgb(14, 16, 26),
            },
            Theme::Dark => ThemeColors {
                bg_dark: Color::Rgb(0, 0, 0),
                bg_mid: Color::Rgb(8, 8, 12),
                bg_light: Color::Rgb(12, 12, 18),
                border: Color::Rgb(30, 30, 40),
                accent: Color::Rgb(0, 150, 220),
                accent_green: Color::Rgb(0, 200, 100),
                accent_red: Color::Rgb(200, 40, 60),
                accent_yellow: Color::Rgb(200, 150, 30),
                accent_purple: Color::Rgb(120, 60, 200),
                text_bright: Color::Rgb(180, 190, 200),
                text_dim: Color::Rgb(70, 75, 90),
                text_faded: Color::Rgb(40, 45, 55),
                bar_cpu: Color::Rgb(0, 150, 220),
                bar_mem: Color::Rgb(200, 140, 0),
                bar_io: Color::Rgb(120, 60, 200),
                row_selected: Color::Rgb(15, 18, 35),
                row_hover: Color::Rgb(10, 12, 22),
            },
            Theme::Light => ThemeColors {
                bg_dark: Color::Rgb(240, 242, 248),
                bg_mid: Color::Rgb(232, 234, 242),
                bg_light: Color::Rgb(225, 228, 238),
                border: Color::Rgb(200, 205, 215),
                accent: Color::Rgb(0, 100, 200),
                accent_green: Color::Rgb(0, 160, 80),
                accent_red: Color::Rgb(200, 30, 50),
                accent_yellow: Color::Rgb(180, 130, 10),
                accent_purple: Color::Rgb(100, 40, 180),
                text_bright: Color::Rgb(20, 25, 35),
                text_dim: Color::Rgb(100, 105, 120),
                text_faded: Color::Rgb(150, 155, 170),
                bar_cpu: Color::Rgb(0, 100, 200),
                bar_mem: Color::Rgb(180, 120, 0),
                bar_io: Color::Rgb(100, 40, 180),
                row_selected: Color::Rgb(210, 215, 230),
                row_hover: Color::Rgb(220, 222, 235),
            },
        }
    }
}

/// All theme colors — accessed via `theme::colors()`
pub struct ThemeColors {
    pub bg_dark: Color,
    pub bg_mid: Color,
    pub bg_light: Color,
    pub border: Color,
    pub accent: Color,
    pub accent_green: Color,
    pub accent_red: Color,
    pub accent_yellow: Color,
    pub accent_purple: Color,
    pub text_bright: Color,
    pub text_dim: Color,
    pub text_faded: Color,
    pub bar_cpu: Color,
    pub bar_mem: Color,
    pub bar_io: Color,
    pub row_selected: Color,
    pub row_hover: Color,
}

// Convenience accessors
pub fn bg_dark() -> Color { colors().bg_dark }
pub fn bg_mid() -> Color { colors().bg_mid }
pub fn bg_light() -> Color { colors().bg_light }
pub fn border() -> Color { colors().border }
pub fn accent() -> Color { colors().accent }
pub fn accent_green() -> Color { colors().accent_green }
pub fn accent_red() -> Color { colors().accent_red }
pub fn accent_yellow() -> Color { colors().accent_yellow }
pub fn accent_purple() -> Color { colors().accent_purple }
pub fn text_bright() -> Color { colors().text_bright }
pub fn text_dim() -> Color { colors().text_dim }
pub fn text_faded() -> Color { colors().text_faded }
pub fn bar_cpu() -> Color { colors().bar_cpu }
pub fn bar_mem() -> Color { colors().bar_mem }
pub fn row_selected() -> Color { colors().row_selected }
pub fn row_hover() -> Color { colors().row_hover }

fn parse_hex_color(hex: &str) -> Result<Color, ()> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 { return Err(()); }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| ())?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| ())?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| ())?;
    Ok(Color::Rgb(r, g, b))
}

pub fn format_uptime(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{}h{:02}m", h, m)
    } else if m > 0 {
        format!("{}m{:02}s", m, s)
    } else {
        format!("{}s", s)
    }
}

pub fn human_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.0}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

pub fn render_scrollbar(area: Rect, buf: &mut ratatui::prelude::Buffer, content_len: usize, viewport_len: usize, offset: usize) {
    if content_len <= viewport_len || viewport_len == 0 {
        return;
    }

    let track_len = area.height.max(2) as usize - 2;
    let thumb_len = ((viewport_len as f64 / content_len as f64) * track_len as f64).max(1.0) as usize;
    let thumb_pos = if content_len > viewport_len {
        ((offset as f64 / (content_len - viewport_len) as f64) * (track_len - thumb_len) as f64) as usize
    } else {
        0
    };

    let x = area.x + area.width - 1;
    for y in 0..area.height {
        if let Some(cell) = buf.cell_mut((x, area.y + y)) {
            if y == 0 || y == area.height - 1 {
                cell.set_char('║')
                    .set_fg(Color::Rgb(35, 40, 55));
            } else if (y as usize - 1) >= thumb_pos && (y as usize - 1) < thumb_pos + thumb_len {
                cell.set_char('█')
                    .set_fg(Color::Rgb(0, 180, 220));
            } else {
                cell.set_char('░')
                    .set_fg(Color::Rgb(30, 33, 48));
            }
        }
    }
}
