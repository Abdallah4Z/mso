use crate::tui::app::App;
use crate::tui::theme;
use crate::protocol::StreamKind;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn render(app: &App, f: &mut Frame, area: Rect) {
    if area.width < 10 || area.height < 3 { return; }

    // Title
    let title = if let Some(proc) = app.selected_process() {
        let name = proc.command.first().map(|s| s.as_str()).unwrap_or("???");
        let count = app.log_total_lines.max(app.log_lines.len());
        let follow = if app.log_follow_mode { " ◆FOLLOW" } else { " ◇PAUSED" };
        format!(" Logs — {name} (PID {}) — {} lines{} ", proc.pid, count, follow)
    } else {
        " Logs — (no process selected) ".to_string()
    };

    let title_y = area.y;
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

    // Content area
    let content_area = Rect::new(area.x, divider_y + 1, area.width, area.height.saturating_sub(2));

    let mut all_lines: Vec<Line> = Vec::new();
    for tl in &app.log_lines {
        let segments = parse_ansi(&tl.line);
        let mut spans = Vec::new();
        let ts = format_timestamp(tl.timestamp);
        spans.push(Span::styled(format!(" {} ", ts), Style::default().fg(theme::text_dim())));
        spans.push(Span::styled("│", Style::default().fg(theme::text_faded())));
        spans.push(Span::raw(" "));
        for (style, text) in &segments {
            spans.push(Span::styled(text.clone(), *style));
        }
        all_lines.push(Line::from(spans));
    }

    let viewport_lines = content_area.height as usize;

    let mut search_area = None;
    if app.search_active {
        search_area = Some(Rect::new(content_area.x, content_area.y + content_area.height - 1, content_area.width, 1));
    }

    let paragraph = Paragraph::new(all_lines).scroll((app.log_scroll_offset as u16, 0));
    f.render_widget(paragraph, content_area);
    theme::render_scrollbar(content_area, f.buffer_mut(), app.log_lines.len(), viewport_lines, app.log_scroll_offset);

    if let Some(sr) = search_area {
        let match_info = if app.search_results.is_empty() { String::new() } else { format!(" ({}/{})", app.search_idx + 1, app.search_results.len()) };
        let filter_info = match app.filter_stream {
            Some(StreamKind::Stdout) => " [stdout]",
            Some(StreamKind::Stderr) => " [stderr]",
            None => "",
        };
        f.render_widget(
            Paragraph::new(format!(" /{}{}{} ", app.search_query, match_info, filter_info))
                .style(Style::default().bg(theme::row_selected()).fg(theme::accent())),
            sr,
        );
    }
}

fn format_timestamp(ts: u64) -> String {
    let secs = ts / 1000;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

fn parse_ansi(line: &str) -> Vec<(Style, String)> {
    let mut segments: Vec<(Style, String)> = Vec::new();
    let mut current = Style::default().fg(Color::Rgb(200, 210, 220));
    let mut bold = false;
    let mut dim = false;
    let mut fg: Option<Color> = None;
    let mut bg: Option<Color> = None;

    let mut remaining = line;
    while let Some(pos) = remaining.find('\x1b') {
        let before = &remaining[..pos];
        if !before.is_empty() { segments.push((current, before.to_string())); }

        let after = &remaining[pos..];
        let end = after[1..].find(|c: char| c.is_ascii_alphabetic() && c != '[');
        match end {
            Some(len) => {
                let seq = &after[1..=len];
                remaining = &after[len + 2..];
                if seq.starts_with('[') {
                    let params: Vec<&str> = seq.strip_prefix('[').unwrap_or(seq).split(';').collect();
                    apply_sgr_params(&params, &mut bold, &mut dim, &mut fg, &mut bg);
                }
                current = Style::default();
                if let Some(c) = fg { current = current.fg(c); }
                if let Some(c) = bg { current = current.bg(c); }
                if bold { current = current.add_modifier(Modifier::BOLD); }
                if dim { current = current.add_modifier(Modifier::DIM); }
            }
            None => break,
        }
    }

    if !remaining.is_empty() { segments.push((current, remaining.to_string())); }

    if segments.is_empty() {
        let style = if line.contains("ERROR") || line.contains("error") { Style::default().fg(Color::Rgb(255, 80, 80)) }
            else if line.contains("WARN") || line.contains("warn") { Style::default().fg(Color::Rgb(255, 200, 50)) }
            else if line.contains("INFO") || line.contains("info") { Style::default().fg(Color::Rgb(80, 255, 140)) }
            else if line.contains("DEBUG") || line.contains("debug") { Style::default().fg(Color::DarkGray) }
            else { Style::default().fg(Color::Rgb(200, 210, 220)) };
        segments.push((style, line.to_string()));
    }
    segments
}

fn apply_sgr_params(params: &[&str], bold: &mut bool, dim: &mut bool, fg: &mut Option<Color>, bg: &mut Option<Color>) {
    for param in params {
        match *param {
            "0" => { *bold = false; *dim = false; *fg = None; *bg = None; }
            "1" => { *bold = true; }
            "2" => { *dim = true; }
            "22" => { *bold = false; *dim = false; }
            "30" => { *fg = Some(Color::Black); }
            "31" => { *fg = Some(Color::Red); }
            "32" => { *fg = Some(Color::Green); }
            "33" => { *fg = Some(Color::Yellow); }
            "34" => { *fg = Some(Color::Blue); }
            "35" => { *fg = Some(Color::Magenta); }
            "36" => { *fg = Some(Color::Cyan); }
            "37" => { *fg = Some(Color::White); }
            "90" => { *fg = Some(Color::DarkGray); }
            "91" => { *fg = Some(Color::LightRed); }
            "92" => { *fg = Some(Color::LightGreen); }
            "93" => { *fg = Some(Color::LightYellow); }
            "94" => { *fg = Some(Color::LightBlue); }
            "95" => { *fg = Some(Color::LightMagenta); }
            "96" => { *fg = Some(Color::LightCyan); }
            "97" => { *fg = Some(Color::White); }
            "40" => { *bg = Some(Color::Black); }
            "41" => { *bg = Some(Color::Red); }
            "42" => { *bg = Some(Color::Green); }
            "43" => { *bg = Some(Color::Yellow); }
            "44" => { *bg = Some(Color::Blue); }
            "45" => { *bg = Some(Color::Magenta); }
            "46" => { *bg = Some(Color::Cyan); }
            "47" => { *bg = Some(Color::White); }
            "38" => {
                let idx = params.iter().position(|&p| p == "38");
                if let Some(idx) = idx {
                    if idx + 2 < params.len() && params[idx + 1] == "5" {
                        if let Ok(n) = params[idx + 2].parse::<u8>() { *fg = Some(Color::Indexed(n)); }
                    }
                }
            }
            _ => {}
        }
    }
}
