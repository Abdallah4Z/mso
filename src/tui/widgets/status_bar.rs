use crate::tui::app::App;
use crate::tui::theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn render(app: &App, f: &mut Frame, area: Rect) {
    let total = app.processes.len();
    let running = app
        .processes
        .iter()
        .filter(|p| p.status == crate::protocol::ProcessStatus::Running)
        .count();

    let uptime = theme::format_uptime(app.daemon_uptime_secs);

    let readonly_badge = if app.readonly { " [READONLY]" } else { "" };

    let left = Span::styled(
        format!(
            "{} [j/k] nav{} [t] tag{} [F3] {}  [/] search  [Esc] quit ",
            readonly_badge,
            if app.readonly { "" } else { "  [r] restart  [x] kill" },
            if app.filtered_tag.is_some() { "✓" } else { "" },
            if app.log_follow_mode { "◆follow" } else { "◇pause" },
        ),
        Style::default().fg(theme::text_dim()),
    );

    let filter_info = app.filtered_tag.as_ref().map(|t| format!(" tag:{}", t)).unwrap_or_default();
    let right = Span::styled(
        format!("{}{}/{} running  │ daemon {}", filter_info, running, total, uptime),
        Style::default().fg(theme::text_dim()),
    );

    let bar = Line::from(vec![left, Span::raw(" "), right]);
    let bg = if let Some((_msg, _t)) = &app.toast {
        theme::bg_light()
    } else {
        theme::bg_light()
    };
    let paragraph = Paragraph::new(bar).style(
        Style::default().bg(bg).fg(theme::text_bright()),
    );

    f.render_widget(paragraph, area);

    // Toast overlay (right side of status bar)
    if let Some((msg, _)) = &app.toast {
        let toast_text = format!(" {} ", msg);
        let toast_w = toast_text.len() as u16 + 2;
        let toast_x = area.x + area.width.saturating_sub(toast_w);
        f.render_widget(
            Paragraph::new(toast_text)
                .style(Style::default().bg(theme::accent_green()).fg(Color::Rgb(0, 0, 0)))
                .alignment(Alignment::Center),
            Rect::new(toast_x, area.y, toast_w, 1),
        );
    }
}
