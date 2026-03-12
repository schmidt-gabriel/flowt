use crate::tui::{App, LogLevel};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

impl App {
    pub fn draw_logs_view(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(f.size());

        // Get the currently selected workflow name
        let workflows = self.get_unique_workflows();
        let selected_workflow_name =
            if !workflows.is_empty() && self.selected_workflow < workflows.len() {
                &workflows[self.selected_workflow].name
            } else {
                "System"
            };

        // Main logs area - show only logs for selected workflow
        let logs = self.logs.lock().unwrap();
        let workflow_logs = logs
            .get(selected_workflow_name)
            .cloned()
            .unwrap_or_default();

        let log_items: Vec<Line> = workflow_logs
            .iter()
            .map(|log| {
                let timestamp = log
                    .timestamp
                    .with_timezone(&chrono::Local)
                    .format("%m/%d %H:%M:%S")
                    .to_string();

                let (icon, color) = match log.level {
                    LogLevel::Info => ("INFO", Color::Blue),
                    LogLevel::Warning => ("WARN", Color::Yellow),
                    LogLevel::Error => ("ERROR", Color::Red),
                };

                Line::from(vec![
                    Span::styled(format!("{} ", timestamp), Style::default().fg(Color::White)),
                    Span::styled(format!("{} ", icon), Style::default().fg(color)),
                    Span::styled(log.message.clone(), Style::default().fg(Color::White)),
                ])
            })
            .collect();

        let logs_widget = Paragraph::new(log_items)
            .block(Block::default().borders(Borders::ALL).title(format!(
                " Logs: {} (l to return to workflows) ",
                selected_workflow_name
            )))
            .scroll((self.log_scroll, 0));

        f.render_widget(logs_widget, chunks[0]);

        // Bottom help bar for logs
        let help = Paragraph::new(Line::from(vec![
            Span::styled(" ↑/↓ scroll ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "| PgUp/PgDn fast scroll ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("| w workflows ", Style::default().fg(Color::DarkGray)),
            Span::styled("| q quit ", Style::default().fg(Color::DarkGray)),
        ]));
        f.render_widget(help, chunks[1]);
    }
}
