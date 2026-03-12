use crate::engine::NodeStatus;
use crate::tui::{App, NodeDetailPanel, RunOrScheduled};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use serde_json;

impl App {
    pub fn draw_node_detail_view_with_area(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
            .split(area);

        // Left panel: Node selector
        self.draw_node_selector(f, chunks[0]);

        // Right panel: Selected node details
        self.draw_selected_node_detail(f, chunks[1]);

        // Bottom help bar
        let help_items = if area.width >= 140 {
            vec![
                " ←/→ switch panel ",
                "| ↑/↓ select node/scroll ",
                "| PgUp/PgDn fast scroll ",
                "| Home/End top/bottom ",
                "| Esc back ",
                "| w workflows ",
                "| q quit ",
            ]
        } else if area.width >= 120 {
            vec![
                " ←/→ panel ",
                "| ↑/↓ select/scroll ",
                "| PgUp/PgDn scroll ",
                "| Home/End ",
                "| Esc back ",
                "| w workflows ",
                "| q quit ",
            ]
        } else if area.width >= 90 {
            vec![
                " ←/→ panel ",
                "| ↑/↓ nav/scroll ",
                "| PgUp/PgDn ",
                "| Home/End ",
                "| Esc back ",
                "| q quit ",
            ]
        } else {
            vec![" ←/→ ", "| ↑/↓ ", "| Esc back ", "| q quit "]
        };

        let help = Paragraph::new(Line::from(
            help_items
                .into_iter()
                .map(|text| Span::styled(text, Style::default().fg(Color::DarkGray)))
                .collect::<Vec<_>>(),
        ));

        let help_area = Rect {
            x: 0,
            y: area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        f.render_widget(help, help_area);
    }

    fn draw_node_selector(&self, f: &mut Frame, area: Rect) {
        let workflow_runs = self.get_runs_and_scheduled_for_selected_workflow();
        if let Some(RunOrScheduled::ActualRun(run)) = workflow_runs.get(self.selected_run) {
            let node_items: Vec<ListItem> = run
                .node_results
                .iter()
                .enumerate()
                .map(|(index, result)| {
                    let (icon, color) = match &result.status {
                        NodeStatus::Pending => ("○", Color::DarkGray),
                        NodeStatus::Running => ("⟳", Color::Yellow),
                        NodeStatus::Success => ("✔", Color::Green),
                        NodeStatus::Failed(_) => ("✘", Color::Red),
                        NodeStatus::Skipped => ("-", Color::DarkGray),
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("{:2}. ", index + 1),
                            Style::default().fg(Color::Cyan),
                        ),
                        Span::styled(format!("{} ", icon), Style::default().fg(color)),
                        Span::styled(result.node_id.clone(), Style::default().fg(Color::White)),
                    ]))
                })
                .collect();

            let mut node_state = ListState::default();
            if !run.node_results.is_empty() && self.selected_node < run.node_results.len() {
                node_state.select(Some(self.selected_node));
            }

            let node_list = List::new(node_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!(" Nodes - Run #{} ", run.id))
                        .border_style(
                            if self.node_detail_focused_panel == NodeDetailPanel::NodeList {
                                Style::default().fg(Color::Cyan)
                            } else {
                                Style::default().fg(Color::DarkGray)
                            },
                        ),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                );

            f.render_stateful_widget(node_list, area, &mut node_state);
        }
    }

    fn draw_selected_node_detail(&self, f: &mut Frame, area: Rect) {
        let workflow_runs = self.get_runs_and_scheduled_for_selected_workflow();
        if let Some(RunOrScheduled::ActualRun(run)) = workflow_runs.get(self.selected_run) {
            if let Some(node_result) = run.node_results.get(self.selected_node) {
                let mut detail_lines = vec![];

                // Node header
                detail_lines.push(Line::from(vec![
                    Span::styled("Node: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        node_result.node_id.clone(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));

                // Status
                let (status_text, status_color) = match &node_result.status {
                    NodeStatus::Pending => ("Pending", Color::DarkGray),
                    NodeStatus::Running => ("Running", Color::Yellow),
                    NodeStatus::Success => ("Success", Color::Green),
                    NodeStatus::Failed(_err) => ("Failed", Color::Red),
                    NodeStatus::Skipped => ("Skipped", Color::DarkGray),
                };

                detail_lines.push(Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(status_text, Style::default().fg(status_color)),
                ]));

                // Timing
                detail_lines.push(Line::from(vec![
                    Span::styled("Started: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        node_result
                            .started_at
                            .with_timezone(&chrono::Local)
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string(),
                        Style::default().fg(Color::White),
                    ),
                ]));

                if let Some(finished_at) = node_result.finished_at {
                    detail_lines.push(Line::from(vec![
                        Span::styled("Finished: ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            finished_at
                                .with_timezone(&chrono::Local)
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string(),
                            Style::default().fg(Color::White),
                        ),
                    ]));

                    let duration = finished_at - node_result.started_at;
                    detail_lines.push(Line::from(vec![
                        Span::styled("Duration: ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            format!("{}ms", duration.num_milliseconds()),
                            Style::default().fg(Color::White),
                        ),
                    ]));
                }

                detail_lines.push(Line::from(""));

                // Error details (if failed)
                if let NodeStatus::Failed(err) = &node_result.status {
                    detail_lines.push(Line::from(vec![Span::styled(
                        "Error: ",
                        Style::default().fg(Color::Red),
                    )]));
                    for line in err.lines() {
                        detail_lines.push(Line::from(vec![Span::styled(
                            format!("  {}", line),
                            Style::default().fg(Color::Red),
                        )]));
                    }
                    detail_lines.push(Line::from(""));
                }

                // Shell Output
                if !node_result.output.is_empty() {
                    detail_lines.push(Line::from(vec![Span::styled(
                        "Shell Output:",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )]));
                    detail_lines.push(Line::from("───────────────────"));
                    for line in node_result.output.lines() {
                        detail_lines.push(Line::from(vec![Span::styled(
                            line,
                            Style::default().fg(Color::White),
                        )]));
                    }
                    detail_lines.push(Line::from(""));
                }

                // API Response Data
                if let Some(response_data) = &node_result.response_data {
                    detail_lines.push(Line::from(vec![Span::styled(
                        "API Response:",
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    )]));
                    detail_lines.push(Line::from("───────────────────"));

                    // Pretty print JSON
                    match serde_json::to_string_pretty(response_data) {
                        Ok(pretty_json) => {
                            let json_lines: Vec<Line> = pretty_json
                                .lines()
                                .map(|line| {
                                    Line::from(vec![Span::styled(
                                        line.to_string(),
                                        Style::default().fg(Color::Magenta),
                                    )])
                                })
                                .collect();
                            detail_lines.extend(json_lines);
                        }
                        Err(_) => {
                            detail_lines.push(Line::from(vec![Span::styled(
                                "Failed to format JSON",
                                Style::default().fg(Color::Red),
                            )]));
                        }
                    }
                }

                let paragraph = Paragraph::new(detail_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Node Details ")
                            .border_style(
                                if self.node_detail_focused_panel == NodeDetailPanel::NodeContent {
                                    Style::default().fg(Color::Cyan)
                                } else {
                                    Style::default().fg(Color::DarkGray)
                                },
                            ),
                    )
                    .wrap(Wrap { trim: true })
                    .scroll((self.detail_scroll, 0));

                f.render_widget(paragraph, area);
            }
        }
    }
}
