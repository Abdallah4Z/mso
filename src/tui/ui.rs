use crate::tui::app::App;
use crate::tui::widgets;
use crate::tui::theme;
use ratatui::prelude::*;
use ratatui::widgets::Block;

pub fn render(app: &App, f: &mut Frame) {
    let area = f.area();

    let [body, status_bar] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(1)])
        .areas(area);

    let sidebar_w = app.sidebar_width;
    let [sidebar, log_pane] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(sidebar_w), Constraint::Min(30)])
        .areas(body);

    // Fill background
    f.render_widget(
        Block::default().style(Style::default().bg(theme::bg_dark())),
        area,
    );

    // Render vertical divider at sidebar border
    let divider_x = sidebar_w;
    let _divider_area = Rect::new(divider_x, body.y, 1, body.height);
    for y in 0..body.height {
        if let Some(cell) = f.buffer_mut().cell_mut((divider_x, body.y + y)) {
            cell.set_char('░')
                .set_fg(theme::border());
        }
    }

    // ── Sidebar ──
    widgets::process_list::render(app, f, sidebar);

    // ── Log pane ──
    widgets::log_view::render(app, f, log_pane);

    // ── Status bar ──
    widgets::status_bar::render(app, f, status_bar);

    // ── Detail overlay ──
    if app.detail_mode {
        widgets::detail_pane::render(app, f);
    }

    // ── Notification panel overlay ──
    if app.notif_mode {
        widgets::notif_panel::render(app, f);
    }
}
