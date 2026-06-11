use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

fn make_app() -> mso::tui::app::App {
    mso::tui::app::App::new(
        tokio::sync::mpsc::unbounded_channel().0,
        tokio::sync::mpsc::unbounded_channel().0,
    )
}

#[test]
fn test_process_list_empty() {
    let app = make_app();
    let backend = TestBackend::new(42, 10);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|f| {
        let area = Rect::new(0, 0, 42, 10);
        mso::tui::widgets::process_list::render(&app, f, area);
    }).unwrap();

    let buffer = terminal.backend().buffer();
    let output = buffer_to_string(buffer, 42, 10);
    assert!(output.contains("No processes"), "empty state should show hint");
}

#[test]
fn test_status_bar_empty() {
    let app = make_app();
    let backend = TestBackend::new(80, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|f| {
        let area = Rect::new(0, 0, 80, 1);
        mso::tui::widgets::status_bar::render(&app, f, area);
    }).unwrap();

    let buffer = terminal.backend().buffer();
    let output = buffer_to_string(buffer, 80, 1);
    assert!(output.contains("quit"), "should show quit hint, got: {output:?}");
}

#[test]
fn test_log_view_empty() {
    let app = make_app();
    let backend = TestBackend::new(60, 10);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|f| {
        let area = Rect::new(0, 0, 60, 10);
        mso::tui::widgets::log_view::render(&app, f, area);
    }).unwrap();

    let buffer = terminal.backend().buffer();
    let output = buffer_to_string(buffer, 60, 10);
    assert!(output.contains("no process selected"), "should show hint");
}

#[test]
fn test_detail_pane_empty() {
    let app = make_app();
    let backend = TestBackend::new(80, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|f| {
        mso::tui::widgets::detail_pane::render(&app, f);
    }).unwrap();
    // Should not panic when no process is selected
}

fn buffer_to_string(buf: &ratatui::buffer::Buffer, w: u16, h: u16) -> String {
    let mut s = String::new();
    for y in 0..h {
        for x in 0..w {
            let cell = buf.cell((x, y)).unwrap();
            s.push(cell.symbol().chars().next().unwrap_or(' '));
        }
        if y + 1 < h {
            s.push('\n');
        }
    }
    s
}
