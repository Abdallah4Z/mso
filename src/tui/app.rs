const SIDEBAR_DEFAULT: u16 = 42;
const SIDEBAR_MIN: u16 = 30;
const SIDEBAR_MAX: u16 = 60;
const PAGE_SCROLL: usize = 20;
const SCROLL_STEP: usize = 3;
const TOAST_SECS: u64 = 2;
const LOG_PAGE_SIZE: usize = 100;
const SEARCH_PAGE_SIZE: usize = 200;
const MAX_NOTIFICATIONS: usize = 100;

use crate::protocol::{DaemonMessage, ManagedProcess, StreamKind, TimestampedLine};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum NotifKind { Crash, Restart, HealthFail, Exit, Info }

#[derive(Debug, Clone)]
pub struct Notification {
    pub timestamp: u64,
    pub kind: NotifKind,
    pub message: String,
}

pub enum TuiEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Tick,
    DaemonMsg(DaemonMessage),
}

pub struct App {
    pub processes: Vec<ManagedProcess>,
    pub selected_index: usize,
    pub log_lines: VecDeque<TimestampedLine>,
    pub log_scroll_offset: usize,
    pub log_total_lines: usize,
    pub list_scroll_offset: usize,
    pub should_quit: bool,
    pub write_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub daemon_uptime_secs: u64,
    pub last_tick: Instant,
    pub io_bytes_cache: HashMap<Uuid, u64>,

    // ── Detail overlay ──
    pub detail_mode: bool,

    // ── Log search ──
    pub search_active: bool,
    pub search_query: String,
    pub search_results: Vec<(usize, TimestampedLine)>,
    pub search_idx: usize,
    pub filter_stream: Option<StreamKind>,

    // ── Tag filter ──
    pub all_tags: Vec<String>,
    pub filtered_tag: Option<String>,
    pub tag_filter_mode: bool,

    // ── Resizable sidebar ──
    pub sidebar_width: u16,
    pub resize_dragging: bool,

    // ── Toast ──
    pub toast: Option<(String, Instant)>,

    // ── Kill confirmation ──
    pub confirm_kill: Option<Uuid>,

    // ── Collapsible groups ──
    pub collapsed_groups: std::collections::HashSet<crate::protocol::ProcessStatus>,

    // ── Log follow mode ──
    pub log_follow_mode: bool,

    // ── Read-only ──
    pub readonly: bool,

    // ── Process search ──
    pub proc_search_active: bool,
    pub proc_search_query: String,

    // ── Notifications ──
    pub notifications: VecDeque<Notification>,
    pub notif_mode: bool,

    // ── Process reordering ──
    pub process_order: Vec<usize>,
}

impl App {
    pub fn new(
        _event_tx: mpsc::UnboundedSender<TuiEvent>,
        write_tx: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Self {
        let readonly = crate::tui::readonly();
        Self {
            processes: Vec::new(),
            selected_index: 0,
            log_lines: VecDeque::new(),
            log_scroll_offset: 0,
            log_total_lines: 0,
            list_scroll_offset: 0,
            should_quit: false,
            write_tx,
            daemon_uptime_secs: 0,
            last_tick: Instant::now(),
            io_bytes_cache: HashMap::new(),
            detail_mode: false,
            search_active: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_idx: 0,
            filter_stream: None,
            all_tags: Vec::new(),
            filtered_tag: None,
            tag_filter_mode: false,
            sidebar_width: SIDEBAR_DEFAULT,
            resize_dragging: false,
            toast: None,
            confirm_kill: None,
            collapsed_groups: std::collections::HashSet::new(),
            log_follow_mode: true,
            readonly,
            proc_search_active: false,
            proc_search_query: String::new(),
            notifications: VecDeque::new(),
            notif_mode: false,
            process_order: Vec::new(),
        }
    }

    pub fn handle_mouse(&mut self, me: MouseEvent) {
        if self.detail_mode { return; }

        match me.kind {
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                let col = me.column as i16;
                let row = me.row as i16;

                // Check if clicking in the resize handle zone (sidebar border area)
                let border_col = self.sidebar_width as i16;
                if (col - border_col).abs() <= 1 {
                    self.resize_dragging = true;
                    return;
                }

                // Click in sidebar area
                if col < border_col && row >= 1 {
                    let item_idx = ((row - 1) / 3) as usize;
                    if item_idx < self.processes.len() {
                        self.selected_index = item_idx;
                        self.on_selection_changed();
                    }
                }
            }
            MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                if self.resize_dragging {
                    self.sidebar_width = me.column.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
                }
            }
            MouseEventKind::Up(_) => {
                self.resize_dragging = false;
            }
            MouseEventKind::ScrollDown => {
                self.log_scroll_offset = self.log_scroll_offset.saturating_add(SCROLL_STEP);
            }
            MouseEventKind::ScrollUp => {
                self.log_scroll_offset = self.log_scroll_offset.saturating_sub(SCROLL_STEP);
            }
            _ => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.detail_mode { self.handle_detail_key(key); return; }
        if self.confirm_kill.is_some() { self.handle_confirm_kill(key); return; }
        if self.tag_filter_mode { self.handle_tag_filter_key(key); return; }
        if self.handle_global_quit(key) { return; }
        self.handle_normal_key(key);
    }

    fn handle_detail_key(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('i')) {
            self.detail_mode = false;
        }
    }

    fn handle_confirm_kill(&mut self, key: KeyEvent) {
        if let Some(id) = self.confirm_kill.take() {
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                self.send_command(crate::protocol::ClientMessage::KillProcess(id));
                if let Some(proc) = self.processes.iter().find(|p| p.id == id) {
                    self.show_toast(&format!("Killed PID {}", proc.pid));
                }
            } else {
                self.show_toast("Kill cancelled");
            }
        }
    }

    fn handle_tag_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => { self.tag_filter_mode = false; self.filtered_tag = None; }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let idx = (c as u8 - b'0') as usize;
                if idx == 0 { self.filtered_tag = None; }
                else if let Some(tag) = self.all_tags.get(idx - 1) {
                    self.filtered_tag = Some(tag.clone());
                }
                self.tag_filter_mode = false;
            }
            _ => {}
        }
    }

    fn handle_global_quit(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('q')) {
            self.should_quit = true;
            true
        } else { false }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if self.notif_mode { self.notif_mode = false; return; }
                if self.search_active {
                    self.search_active = false;
                    self.search_query.clear();
                } else {
                    self.should_quit = true;
                }
            }

            KeyCode::Enter if self.search_active => {
                self.execute_search();
            }

            KeyCode::Char('/') if !self.search_active => {
                self.search_active = true;
                self.search_query.clear();
            }

            KeyCode::Char(c) if self.search_active => {
                self.search_query.push(c);
            }

            KeyCode::Backspace if self.search_active => {
                self.search_query.pop();
            }

            KeyCode::Char('n') if self.search_active && !self.search_results.is_empty() => {
                self.search_idx = (self.search_idx + 1) % self.search_results.len();
            }

            KeyCode::Char('N') if self.search_active && !self.search_results.is_empty() => {
                self.search_idx = if self.search_idx == 0 {
                    self.search_results.len() - 1
                } else {
                    self.search_idx - 1
                };
            }

            KeyCode::F(1) => {
                self.filter_stream = match self.filter_stream {
                    Some(StreamKind::Stdout) => None,
                    _ => Some(StreamKind::Stdout),
                };
                if self.search_active { self.execute_search(); }
            }

            KeyCode::F(2) => {
                self.filter_stream = match self.filter_stream {
                    Some(StreamKind::Stderr) => None,
                    _ => Some(StreamKind::Stderr),
                };
                if self.search_active { self.execute_search(); }
            }

            KeyCode::F(3) => {
                self.log_follow_mode = !self.log_follow_mode;
            }

            KeyCode::Up | KeyCode::Char('k') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Reorder: move up
                    if self.selected_index > 0 && self.process_order.len() == self.processes.len() {
                        self.process_order.swap(self.selected_index, self.selected_index - 1);
                        self.selected_index -= 1;
                    }
                } else if self.selected_index > 0 {
                    self.selected_index -= 1;
                    self.on_selection_changed();
                }
            }

            KeyCode::Down | KeyCode::Char('j') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Reorder: move down
                    if self.selected_index + 1 < self.processes.len() && self.process_order.len() == self.processes.len() {
                        self.process_order.swap(self.selected_index, self.selected_index + 1);
                        self.selected_index += 1;
                    }
                } else if self.selected_index + 1 < self.processes.len() {
                    self.selected_index += 1;
                    self.on_selection_changed();
                }
            }

            KeyCode::Home | KeyCode::Char('g') => {
                self.selected_index = 0;
                self.on_selection_changed();
            }

            KeyCode::End | KeyCode::Char('G') => {
                if !self.processes.is_empty() {
                    self.selected_index = self.processes.len() - 1;
                    self.on_selection_changed();
                }
            }

            KeyCode::PageUp => {
                self.log_scroll_offset = self.log_scroll_offset.saturating_add(PAGE_SCROLL);
            }

            KeyCode::PageDown => {
                self.log_scroll_offset = self.log_scroll_offset.saturating_sub(PAGE_SCROLL);
            }

            KeyCode::Enter | KeyCode::Char('i') => {
                if !self.processes.is_empty() {
                    self.detail_mode = true;
                }
            }

            KeyCode::Char('r') => {
                if self.readonly { return; }
                if let Some(proc) = self.processes.get(self.selected_index) {
                    let id = proc.id;
                    self.send_command(crate::protocol::ClientMessage::RestartProcess(id));
                    self.show_toast(&format!("Restarting PID {}", proc.pid));
                }
            }

            KeyCode::Char('x') => {
                if self.readonly { return; }
                if let Some(proc) = self.processes.get(self.selected_index) {
                    self.confirm_kill = Some(proc.id);
                    self.show_toast(&format!("Kill PID {}? [y/N]", proc.pid));
                }
            }

            KeyCode::Backspace if !self.proc_search_active => {
                if self.readonly { return; }
                if let Some(proc) = self.processes.get(self.selected_index) {
                    let id = proc.id;
                    self.send_command(crate::protocol::ClientMessage::KillProcess(id));
                    self.show_toast(&format!("Killing PID {}", proc.pid));
                }
            }

            KeyCode::Char('t') if !self.search_active => {
                self.tag_filter_mode = !self.tag_filter_mode;
                if self.tag_filter_mode {
                    let mut tags: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                    for proc in &self.processes {
                        for tag in &proc.tags {
                            tags.insert(tag.clone());
                        }
                    }
                    self.all_tags = tags.into_iter().collect();
                }
            }

            KeyCode::Char('f') if !self.search_active => {
                self.proc_search_active = !self.proc_search_active;
                if !self.proc_search_active {
                    self.proc_search_query.clear();
                }
            }

            KeyCode::Char(c) if self.proc_search_active => {
                self.proc_search_query.push(c);
            }

            KeyCode::Backspace if self.proc_search_active => {
                self.proc_search_query.pop();
                if self.proc_search_query.is_empty() {
                    self.proc_search_active = false;
                }
            }

            KeyCode::Char('R') => {
                if let Some(proc) = self.processes.get(self.selected_index) {
                    let offset = self.log_lines.len() + self.log_scroll_offset;
                    self.send_command(crate::protocol::ClientMessage::GetLogs {
                        process_id: proc.id,
                        offset,
                        limit: LOG_PAGE_SIZE,
                    });
                }
            }

            KeyCode::Char('N') if !self.search_active => {
                self.notif_mode = !self.notif_mode;
            }

            _ => {}
        }
    }

    fn show_toast(&mut self, msg: &str) {
        self.toast = Some((msg.to_string(), Instant::now()));
    }

    fn execute_search(&mut self) {
        if self.search_query.is_empty() { return; }
        if let Some(proc) = self.processes.get(self.selected_index) {
            self.search_results.clear();
            self.search_idx = 0;
            self.send_command(crate::protocol::ClientMessage::SearchLogs {
                process_id: proc.id,
                query: self.search_query.clone(),
                stream: self.filter_stream,
                offset: 0,
                limit: SEARCH_PAGE_SIZE,
            });
        }
    }

    fn on_selection_changed(&mut self) {
        self.log_scroll_offset = 0;
        self.log_lines.clear();
        self.log_total_lines = 0;
        self.search_results.clear();
        self.search_idx = 0;
        self.log_follow_mode = true;
        if let Some(proc) = self.processes.get(self.selected_index) {
            self.log_lines = proc.log_lines.clone();
        }
    }

    pub async fn on_daemon_msg(&mut self, msg: DaemonMessage) {
        match msg {
            DaemonMessage::StateSnapshot(snap) => {
                self.processes = snap.processes;
                // Initialize process_order if needed
                if self.process_order.len() != self.processes.len() || self.process_order.is_empty() {
                    self.process_order = (0..self.processes.len()).collect();
                }
                for proc in &self.processes {
                    if proc.io_bytes > 0 {
                        self.io_bytes_cache.entry(proc.id).or_insert(proc.io_bytes);
                    }
                }
                // Only copy log lines for the selected process to avoid O(n) clone on every tick
                if let Some(proc) = self.processes.get(self.selected_index) {
                    if self.log_lines.is_empty() || self.selected_process_id() != Some(proc.id) {
                        self.log_lines = proc.log_lines.clone();
                    }
                }
            }
            DaemonMessage::LogEvent { process_id, line, timestamp, .. } => {
                let ts_line = TimestampedLine { timestamp, line: line.clone() };
                if let Some(proc) = self.processes.iter_mut().find(|p| p.id == process_id) {
                    if proc.log_lines.len() >= crate::protocol::MAX_LOG_LINES {
                        proc.log_lines.pop_front();
                    }
                    proc.log_lines.push_back(ts_line.clone());
                }
                if self.selected_process_id() == Some(process_id) {
                    self.log_lines.push_back(ts_line);
                    self.log_scroll_offset = 0;
                }
                // Push notification on error keywords
                if line.contains("ERROR") || line.contains("FATAL") || line.contains("panic") {
                    let name = self.processes.iter().find(|p| p.id == process_id)
                        .and_then(|p| p.command.first().cloned()).unwrap_or_default();
                    self.push_notification(NotifKind::Info, format!("{name}: {line}"));
                }
            }
            DaemonMessage::LogsBatch { logs, total } => {
                for tl in logs.into_iter().rev() {
                    self.log_lines.push_front(tl);
                }
                self.log_total_lines = total;
            }
            DaemonMessage::SearchResult { logs, .. } => {
                self.search_results = logs.into_iter().enumerate().collect();
                self.search_idx = 0;
            }
            DaemonMessage::StatusChanged { process_id, status } => {
                if let Some(proc) = self.processes.iter_mut().find(|p| p.id == process_id) {
                    let old_status = proc.status;
                    proc.status = status;
                    let name = proc.command.first().cloned().unwrap_or_default();
                    let kind = match (old_status, status) {
                        (_, crate::protocol::ProcessStatus::Crashed) => NotifKind::Crash,
                        (crate::protocol::ProcessStatus::Running, crate::protocol::ProcessStatus::Stopped) => NotifKind::Exit,
                        _ => NotifKind::Info,
                    };
                    self.push_notification(kind, format!("{name}: {:?} → {:?}", old_status, status));
                }
            }
            DaemonMessage::HealthStatus { process_id, healthy, failures } => {
                if !healthy {
                    if let Some(proc) = self.processes.iter().find(|p| p.id == process_id) {
                        let name = proc.command.first().cloned().unwrap_or_default();
                        self.push_notification(NotifKind::HealthFail, format!("{name}: health check failed ({failures})"));
                    }
                }
            }
            DaemonMessage::TelemetryUpdate { process_id, telemetry } => {
                if let Some(proc) = self.processes.iter_mut().find(|p| p.id == process_id) {
                    proc.cpu_percent = telemetry.cpu_percent;
                    proc.memory_bytes = telemetry.memory_bytes;
                    proc.ports = telemetry.ports;
                    self.io_bytes_cache.insert(process_id, telemetry.io_bytes);
                    proc.io_bytes = telemetry.io_bytes;
                }
            }
            _ => {}
        }
    }

    fn push_notification(&mut self, kind: NotifKind, message: String) {
        self.notifications.push_back(Notification {
            timestamp: crate::util::epoch_millis(),
            kind,
            message,
        });
        while self.notifications.len() > MAX_NOTIFICATIONS {
            self.notifications.pop_front();
        }
    }

    pub fn on_tick(&mut self) {
        let now = Instant::now();
        self.daemon_uptime_secs += now.duration_since(self.last_tick).as_secs();
        self.last_tick = now;

        // Clear toast after 2s
        if let Some((_, t)) = &self.toast {
            if now.duration_since(*t).as_secs() > TOAST_SECS {
                self.toast = None;
            }
        }

        // Responsive sidebar width
        // (handled in handle_key on resize, or here as a heuristic)
        // For now, just let the user set it via drag

        for proc in &mut self.processes {
            if proc.status == crate::protocol::ProcessStatus::Running {
                let elapsed = (crate::util::epoch_millis() - proc.started_at) / 1000;
                proc.uptime_secs = elapsed;
            }
        }

        self.send_command(crate::protocol::ClientMessage::GetState);
    }

    fn selected_process_id(&self) -> Option<Uuid> {
        self.processes.get(self.selected_index).map(|p| p.id)
    }

    fn send_command(&self, msg: crate::protocol::ClientMessage) {
        if let Ok(wire) = crate::protocol::encode_message(&msg) {
            let _ = self.write_tx.send(wire);
        }
    }

    pub fn selected_process(&self) -> Option<&ManagedProcess> {
        self.processes.get(self.selected_index)
    }

    pub fn displayed_processes(&self) -> Vec<&ManagedProcess> {
        match &self.filtered_tag {
            Some(tag) => self.processes.iter().filter(|p| p.tags.contains(tag)).collect(),
            None => self.processes.iter().collect(),
        }
    }

    /// Returns indices of processes matching the search query and tag filter
    pub fn filtered_process_indices(&self) -> Vec<usize> {
        self.processes.iter().enumerate().filter(|(_, p)| {
            let matches_tag = match &self.filtered_tag {
                Some(tag) => p.tags.contains(tag),
                None => true,
            };
            let matches_query = if self.proc_search_query.is_empty() {
                true
            } else {
                let q = self.proc_search_query.to_lowercase();
                p.command.first().map(|s| s.to_lowercase().contains(&q)).unwrap_or(false)
                    || p.pid.to_string().contains(&q)
                    || p.tags.iter().any(|t| t.to_lowercase().contains(&q))
            };
            matches_tag && matches_query
        }).map(|(i, _)| i).collect()
    }
}
