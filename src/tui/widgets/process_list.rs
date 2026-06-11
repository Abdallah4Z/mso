use crate::protocol::ProcessStatus;
use crate::tui::app::App;
use crate::tui::theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn render(app: &App, f: &mut Frame, area: Rect) {
    if area.width < 5 || area.height < 3 { return; }

    // Title
    let title_y = area.y;
    f.buffer_mut().cell_mut((area.x, title_y)).map(|c| c.set_char(' '));
    let title = format!(" Processes ({}) ", app.processes.len());
    for (i, ch) in title.chars().enumerate() {
        if let Some(cell) = f.buffer_mut().cell_mut((area.x + i as u16, title_y)) {
            cell.set_char(ch).set_fg(theme::text_dim()).set_bg(theme::bg_dark());
        }
    }

    // Thin divider line below title
    let divider_y = title_y + 1;
    for x in area.x..area.x + area.width {
        if let Some(cell) = f.buffer_mut().cell_mut((x, divider_y)) {
            cell.set_char('─').set_fg(theme::border());
        }
    }

    // Content area starts after title + divider
    let content_area = Rect::new(area.x, divider_y + 1, area.width, area.height.saturating_sub(2));

    if app.processes.is_empty() {
        let hint = Paragraph::new(" No processes\n\n Run `mso run <cmd>`\n to get started")
            .style(Style::default().fg(theme::text_dim()))
            .alignment(Alignment::Center);
        f.render_widget(hint, content_area);
        return;
    }

    let item_height: usize = 3;
    let viewport_count = (content_area.height as usize).max(1) / item_height;

    let mut display_idx = 0usize;
    let mut items: Vec<ListItem> = Vec::new();
    let mut add_group = |status: ProcessStatus, label: &str| {
        let indices: Vec<usize> = app.processes.iter()
            .enumerate().filter(|(_, p)| p.status == status).map(|(i, _)| i).collect();
        if indices.is_empty() { return; }

        let header = Line::from(Span::styled(
            format!(" {} ({})", label, indices.len()),
            Style::default().fg(theme::text_dim()),
        ));
        items.push(ListItem::new(vec![header]).style(Style::default().bg(theme::bg_light())));
        display_idx += 1;

        for &proc_idx in &indices {
            if let Some(proc) = app.processes.get(proc_idx) {
                let status_icon = match proc.status {
                    ProcessStatus::Running => "◉", ProcessStatus::Sleeping => "○",
                    ProcessStatus::Crashed => "✕", ProcessStatus::Stopped => "■",
                };
                let status_color = match proc.status {
                    ProcessStatus::Running => theme::accent_green(), ProcessStatus::Sleeping => theme::accent_yellow(),
                    ProcessStatus::Crashed => theme::accent_red(), ProcessStatus::Stopped => theme::text_dim(),
                };

                let name = proc.command.first().map(|s| s.as_str()).unwrap_or("???");
                let dir_name = proc.working_dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
                let name_with_dir = if dir_name.is_empty() { name.to_string() } else { format!("{} ({})", name, dir_name) };
                let name_display = if name_with_dir.len() > 20 { format!("{}…", &name_with_dir[..19]) } else { name_with_dir };
                let uptime_str = format_compact_uptime(proc.uptime_secs);
                let tag_str = if proc.tags.is_empty() { String::new() } else { format!(" [{}]", proc.tags.join(",")) };
                let health_str = if proc.health_ok { " ✓" } else if proc.health_check.is_some() { " ✗" } else { "" };

                let line1 = Line::from(vec![
                    Span::styled(format!(" {} ", status_icon), Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("{:<18}", name_display), Style::default().fg(theme::text_bright())),
                    Span::styled(health_str, Style::default().fg(if proc.health_ok { theme::accent_green() } else { theme::accent_red() })),
                    Span::styled(tag_str, Style::default().fg(theme::accent_purple()).add_modifier(Modifier::DIM)),
                    Span::styled(format!(" {}", uptime_str), Style::default().fg(theme::text_dim())),
                ]);

                let cpu_str = format!("{:.0}%", proc.cpu_percent);
                let mem_str = theme::human_bytes(proc.memory_bytes);
                let io_str = theme::human_bytes(proc.io_bytes);

                let line2 = Line::from(vec![
                    Span::styled(format!(" PID {:<6}", proc.pid), Style::default().fg(theme::text_dim())),
                    Span::styled(format!("{:<5}", cpu_str), Style::default().fg(theme::text_bright())),
                    Span::styled(format!(" {} ", proc.sparkline_cpu), Style::default().fg(theme::bar_cpu()).add_modifier(Modifier::DIM)),
                    Span::styled(format!(" {:<8}", mem_str), Style::default().fg(theme::text_dim())),
                    Span::styled(format!(" I/O {}", io_str), Style::default().fg(theme::text_faded())),
                ]);

                let line3 = if !proc.ports.is_empty() {
                    let ports_str = proc.ports.iter().map(|p| format!(":{}", p)).collect::<Vec<_>>().join(" ");
                    Line::from(vec![Span::styled(format!("      {} ", ports_str), Style::default().fg(theme::accent()).add_modifier(Modifier::DIM))])
                } else { Line::from(Span::raw("")) };

                let mut item = ListItem::new(vec![line1, line2, line3]);
                if proc_idx == app.selected_index {
                    item = item.style(Style::default().bg(theme::row_selected()));
                } else if display_idx.is_multiple_of(2) {
                    item = item.style(Style::default().bg(theme::row_hover()));
                }
                items.push(item);
                display_idx += 1;
            }
        }
    };

    add_group(ProcessStatus::Running, "Running");
    add_group(ProcessStatus::Sleeping, "Sleeping");
    add_group(ProcessStatus::Crashed, "Crashed");
    add_group(ProcessStatus::Stopped, "Stopped");

    let list = List::new(items).highlight_style(Style::default().bg(theme::row_selected()));
    f.render_widget(list, content_area);
    theme::render_scrollbar(content_area, f.buffer_mut(), display_idx, viewport_count, app.list_scroll_offset);
}

fn format_compact_uptime(secs: u64) -> String {
    if secs == 0 { String::new() } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if h > 0 { format!("↑{}h{:02}m", h, m) }
        else if m > 0 { format!("↑{}m", m) }
        else { format!("↑{}s", secs) }
    }
}
