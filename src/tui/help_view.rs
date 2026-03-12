use crate::tui::App;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

impl App {
    pub fn draw_help_view(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(f.size());

        // Build help content
        let help_lines = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "Flowt - Workflow Automation Tool",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Navigation:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from("  ↑/↓/j/k     Navigate up/down in lists and scroll in detail views"),
            Line::from("  PgUp/PgDn    Fast scroll in logs, descriptions, and help"),
            Line::from("  Tab/←/→      Switch focus between panels (Workflows → Runs → Results)"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Workflow Management:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from("  Space/Enter  Toggle workflow enabled/disabled state"),
            Line::from("  Enter        View detailed node results (when focused on runs or results panel)"),
            Line::from("  t            Trigger the selected workflow manually"),
            Line::from("  r            Refresh workflow list and data"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Views:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from("  l            Switch to logs view (from workflows)"),
            Line::from("  d            Show description of selected workflow"),
            Line::from("  w            Return to main workflows view (from any view)"),
            Line::from("  ?            Show this help screen"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Node Detail View:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from("  Enter        Enter detail view (from results panel)"),
            Line::from("  Esc          Return to workflows view"),
            Line::from("  ←/→          Switch between node list and content panels"),
            Line::from("  ↑/↓          Select nodes (left panel) or scroll content (right panel)"),
            Line::from("  PgUp/PgDn    Fast scroll through content"),
            Line::from("  Home/End     Jump to top/bottom of content"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Workflow Status Icons:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  ● ", Style::default().fg(Color::Green)),
                Span::styled("Enabled    ", Style::default().fg(Color::White)),
                Span::styled("○ ", Style::default().fg(Color::DarkGray)),
                Span::styled("Disabled", Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Run Status Icons:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  ✔ ", Style::default().fg(Color::Green)),
                Span::styled("Success   ", Style::default().fg(Color::White)),
                Span::styled("✘ ", Style::default().fg(Color::Red)),
                Span::styled("Failed   ", Style::default().fg(Color::White)),
                Span::styled("⟳ ", Style::default().fg(Color::Yellow)),
                Span::styled("Running", Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  🕐 ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    "Scheduled (for cron workflows)",
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Node Status Icons:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  ○ ", Style::default().fg(Color::DarkGray)),
                Span::styled("Pending   ", Style::default().fg(Color::White)),
                Span::styled("⟳ ", Style::default().fg(Color::Yellow)),
                Span::styled("Running   ", Style::default().fg(Color::White)),
                Span::styled("✔ ", Style::default().fg(Color::Green)),
                Span::styled("Success   ", Style::default().fg(Color::White)),
                Span::styled("✘ ", Style::default().fg(Color::Red)),
                Span::styled("Failed", Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Trigger Types:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from("  manual       Triggered manually with 't' key"),
            Line::from("  cron         Automatically triggered on schedule"),
            Line::from("  webhook      Triggered by HTTP requests"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Tips:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from("  • Workflows are loaded from YAML files in the workflows directory"),
            Line::from("  • Use Space to enable/disable workflows"),
            Line::from("  • Focus the runs panel (Tab/→) and press Enter for detailed view"),
            Line::from("  • Detail view shows full shell output and JSON API responses"),
            Line::from("  • Cron workflows show their next scheduled run time"),
            Line::from("  • Logs are specific to each workflow"),
            Line::from("  • Use 'r' to refresh if you've added new workflow files"),
            Line::from(""),
        ];

        let help_widget = Paragraph::new(help_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Help (w to return to workflows) "),
            )
            .wrap(Wrap { trim: true })
            .scroll((self.detail_scroll, 0));

        f.render_widget(help_widget, chunks[0]);

        // Bottom help bar for help screen
        let help = Paragraph::new(Line::from(vec![
            Span::styled(" ↑/↓ scroll ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "| PgUp/PgDn fast scroll ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("| w workflows ", Style::default().fg(Color::DarkGray)),
            Span::styled("| Esc return ", Style::default().fg(Color::DarkGray)),
            Span::styled("| q quit ", Style::default().fg(Color::DarkGray)),
        ]));
        f.render_widget(help, chunks[1]);
    }
}
