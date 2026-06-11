use crate::tui::app::{App, NotifKind};
use crate::tui::theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn render(app: &App, f: &mut Frame) {
    let area = f.area();
    let overlay_w = (area.width as f64 * 0.60).min(60.0) as u16;
    let overlay_h = (area.height as f64 * 0.60).min(25.0) as u16;
    let x = (area.width - overlay_w) / 2;
    let y = (area.height - overlay_h) / 2;
    let rect = Rect::new(x, y, overlay_w, overlay_h);

    f.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Notifications ")
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(theme::accent()))
        .style(Style::default().bg(theme::bg_mid()));

    let _inner = block.inner(rect);

    let items: Vec<Line> = app.notifications.iter().rev().take(20).map(|n| {
        let (icon, color) = match n.kind {
            NotifKind::Crash => ("✕", theme::accent_red()),
            NotifKind::Restart => ("↻", theme::accent_green()),
            NotifKind::HealthFail => ("⚠", theme::accent_yellow()),
            NotifKind::Exit => ("■", theme::text_dim()),
            NotifKind::Info => ("●", theme::accent()),
        };
        let ts = {
            let secs = n.timestamp / 1000;
            let h = (secs / 3600) % 24;
            let m = (secs / 60) % 60;
            let s = secs % 60;
            format!("{:02}:{:02}:{:02}", h, m, s)
        };
        Line::from(vec![
            Span::styled(format!(" {icon} "), Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {ts} "), Style::default().fg(theme::text_dim())),
            Span::styled(&n.message, Style::default().fg(theme::text_bright())),
        ])
    }).collect();

    let paragraph = Paragraph::new(items).block(block);
    f.render_widget(paragraph, rect);

    let hint = Line::from(Span::styled(" [Esc] close  [N] close ", Style::default().fg(theme::text_dim())));
    f.render_widget(Paragraph::new(hint).style(Style::default().bg(theme::bg_light())), Rect::new(x, y + overlay_h - 1, overlay_w, 1));
}
