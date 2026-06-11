use crate::protocol::ProcessStatus;
use crate::tui::app::App;
use crate::tui::theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn render(app: &App, f: &mut Frame) {
    let area = f.area();
    let Some(proc) = app.selected_process() else { return };

    let overlay_w = (area.width as f64 * 0.70).min(80.0) as u16;
    let overlay_h = (area.height as f64 * 0.75).min(30.0) as u16;
    let x = (area.width - overlay_w) / 2;
    let y = (area.height - overlay_h) / 2;
    let rect = Rect::new(x, y, overlay_w, overlay_h);

    f.render_widget(Clear, rect);
    f.render_widget(Block::default().style(Style::default().bg(theme::bg_mid())), rect);

    // Title line
    let title = format!(" Process Card — PID {} ", proc.pid);
    for (i, ch) in title.chars().enumerate() {
        if let Some(cell) = f.buffer_mut().cell_mut((rect.x + i as u16, rect.y)) {
            cell.set_char(ch).set_fg(theme::accent()).set_bg(theme::bg_mid());
        }
    }

    // Divider line below title
    for xp in rect.x..rect.x + rect.width {
        if let Some(cell) = f.buffer_mut().cell_mut((xp, rect.y + 1)) {
            cell.set_char('─').set_fg(theme::border()).set_bg(theme::bg_mid());
        }
    }

    // Content area (below title + divider)
    let inner = Rect::new(rect.x, rect.y + 2, rect.width, rect.height.saturating_sub(2));

    // Split into left (metrics) and right (metadata)
    let col_w = inner.width / 2;
    let [left_col, right_col] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(col_w), Constraint::Min(10)])
        .areas(inner);

    // ── LEFT COLUMN: Identity + Resources ──
    let status_style = match proc.status {
        ProcessStatus::Running => Style::default().fg(theme::accent_green()),
        ProcessStatus::Sleeping => Style::default().fg(theme::accent_yellow()),
        ProcessStatus::Crashed => Style::default().fg(theme::accent_red()),
        ProcessStatus::Stopped => Style::default().fg(theme::text_dim()),
    };

    let status_icon = match proc.status {
        ProcessStatus::Running => "◉",
        ProcessStatus::Sleeping => "○",
        ProcessStatus::Crashed => "✕",
        ProcessStatus::Stopped => "■",
    };

    let name = proc.command.first().map(|s| s.as_str()).unwrap_or("???");
    let health_str = if proc.health_check.is_some() {
        if proc.health_ok { "  ✓ Healthy" } else { "  ✗ Unhealthy" }
    } else { "" };

    // Header: icon + name + status
    let header_lines = vec![
        Line::from(vec![
            Span::styled(format!(" {}  {}", status_icon, name), status_style.add_modifier(Modifier::BOLD)),
            Span::styled(health_str, Style::default().fg(if proc.health_ok { theme::accent_green() } else { theme::accent_red() })),
        ]),
        Line::from(vec![
            Span::styled(format!(" PID {}", proc.pid), Style::default().fg(theme::text_dim())),
            Span::styled(format!("  Restart: {:?}", proc.restart_policy), Style::default().fg(theme::text_dim())),
        ]),
        Line::from(Span::raw("")),
    ];

    // Resources section
    let cpu_bar_w = 20usize;
    let cpu_fill = (proc.cpu_percent / 100.0 * cpu_bar_w as f32).round() as usize;
    let cpu_bar: String = (0..cpu_bar_w).map(|j| if j < cpu_fill { '█' } else { '░' }).collect();

    let mem_bar_w = 20usize;
    let mem_pct = if proc.memory_bytes > 0 {
        ((proc.memory_bytes as f64 / (1024.0 * 1024.0 * 1024.0)) * mem_bar_w as f64).min(mem_bar_w as f64) as usize
    } else { 0 };
    let mem_bar: String = (0..mem_bar_w).map(|j| if j < mem_pct { '█' } else { '░' }).collect();

    let resource_lines = vec![
        Line::from(Span::styled(" ── Resources ──", Style::default().fg(theme::text_dim()))),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled(" CPU  ", Style::default().fg(theme::text_dim())),
            Span::styled(format!("{} ", cpu_bar), Style::default().fg(theme::bar_cpu())),
            Span::styled(format!("{:.0}%", proc.cpu_percent), Style::default().fg(theme::text_bright())),
            Span::styled(format!("  {}", proc.sparkline_cpu), Style::default().fg(theme::bar_cpu()).add_modifier(Modifier::DIM)),
        ]),
        Line::from(vec![
            Span::styled(" MEM  ", Style::default().fg(theme::text_dim())),
            Span::styled(format!("{} ", mem_bar), Style::default().fg(theme::bar_mem())),
            Span::styled(theme::human_bytes(proc.memory_bytes), Style::default().fg(theme::text_bright())),
        ]),
        Line::from(vec![
            Span::styled(" I/O  ", Style::default().fg(theme::text_dim())),
            Span::styled(theme::human_bytes(proc.io_bytes), Style::default().fg(theme::text_bright())),
            Span::styled(format!("  Ports: {}", if proc.ports.is_empty() { "none".into() } else { proc.ports.iter().map(|p| format!(":{}", p)).collect::<Vec<_>>().join(" ") }),
                Style::default().fg(theme::accent())),
        ]),
        Line::from(Span::raw("")),
    ];

    let left_items: Vec<Line> = header_lines.into_iter().chain(resource_lines).collect();
    let left_para = Paragraph::new(left_items).wrap(Wrap { trim: false });

    // ── RIGHT COLUMN: Details + Environment ──
    let cmd_str = proc.command.join(" ");
    let wd_str = proc.working_dir.display().to_string();
    let uptime = theme::format_uptime(proc.uptime_secs);
    let started = format_timestamp(proc.started_at);
    let tags_str = if proc.tags.is_empty() { "none".into() } else { proc.tags.join(", ") };

    let env_lines: Vec<String> = proc.env_vars.iter()
        .take(12)
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    let env_show = if proc.env_vars.len() > 12 {
        format!("{} (+{} more)", env_lines.join("\n"), proc.env_vars.len() - 12)
    } else {
        env_lines.join("\n")
    };

    let right_items = vec![
        Line::from(Span::styled(" ── Details ──", Style::default().fg(theme::text_dim()))),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled("Command  ", Style::default().fg(theme::text_dim())),
            Span::styled(cmd_str, Style::default().fg(theme::text_bright())),
        ]),
        Line::from(vec![
            Span::styled("Dir      ", Style::default().fg(theme::text_dim())),
            Span::styled(wd_str, Style::default().fg(theme::text_dim())),
        ]),
        Line::from(vec![
            Span::styled("Uptime   ", Style::default().fg(theme::text_dim())),
            Span::styled(uptime, Style::default().fg(theme::text_bright())),
        ]),
        Line::from(vec![
            Span::styled("Started  ", Style::default().fg(theme::text_dim())),
            Span::styled(started, Style::default().fg(theme::text_bright())),
        ]),
        Line::from(vec![
            Span::styled("Tags     ", Style::default().fg(theme::text_dim())),
            Span::styled(tags_str, Style::default().fg(theme::accent_purple())),
        ]),
        Line::from(Span::raw("")),
        Line::from(Span::styled(" ── Environment (first 12) ──", Style::default().fg(theme::text_dim()))),
        Line::from(Span::raw("")),
        Line::from(Span::styled(env_show, Style::default().fg(theme::text_faded()))),
    ];

    let right_para = Paragraph::new(right_items).wrap(Wrap { trim: false });

    // Content
    f.render_widget(left_para, left_col);
    f.render_widget(right_para, right_col);

    // Bottom hint
    let hint = Line::from(Span::styled(" [Esc] close  [i] close ", Style::default().fg(theme::text_dim())));
    f.render_widget(Paragraph::new(hint).style(Style::default().bg(theme::bg_light())), Rect::new(x, y + overlay_h - 1, overlay_w, 1));
}

fn format_timestamp(ts: u64) -> String {
    let secs = ts / 1000;
    let days = secs / 86400;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}+{:02}:{:02}:{:02}", days, h, m, s)
}
