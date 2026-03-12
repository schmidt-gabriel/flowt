use crate::config::{NodeKind, TriggerConfig};
use crate::tui::App;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

impl App {
    pub fn draw_description_view(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(f.size());

        // Get the currently selected workflow
        let workflows = self.get_unique_workflows();
        let (workflow_name, description, triggers_info) =
            if !workflows.is_empty() && self.selected_workflow < workflows.len() {
                let workflow = &workflows[self.selected_workflow];
                let triggers_text = if workflow.triggers.is_empty() {
                    "No triggers configured".to_string()
                } else {
                    workflow
                        .triggers
                        .iter()
                        .map(|trigger| match trigger {
                            TriggerConfig::Manual => "Manual trigger".to_string(),
                            TriggerConfig::Cron { schedule } => format!("Cron: {}", schedule),
                            TriggerConfig::Webhook { port } => format!("Webhook on port {}", port),
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                };

                let nodes_count = workflow.nodes.len();
                let nodes_info = if nodes_count == 0 {
                    "No nodes configured".to_string()
                } else {
                    format!(
                        "{} node{}",
                        nodes_count,
                        if nodes_count == 1 { "" } else { "s" }
                    )
                };

                (
                    workflow.name.clone(),
                    if workflow.description.is_empty() {
                        "No description provided".to_string()
                    } else {
                        workflow.description.clone()
                    },
                    format!("{} | {}", triggers_text, nodes_info),
                )
            } else {
                (
                    "No workflow selected".to_string(),
                    "Please select a workflow first.".to_string(),
                    "".to_string(),
                )
            };

        // Build description content
        let mut description_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("Workflow: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    workflow_name,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Configuration: ", Style::default().fg(Color::DarkGray)),
                Span::styled(triggers_info, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Description:",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
        ];

        // Split description into lines for better display
        for desc_line in description.lines() {
            description_lines.push(Line::from(desc_line.to_string()));
        }

        // Add nodes section if workflow is selected
        if !workflows.is_empty() && self.selected_workflow < workflows.len() {
            let workflow = &workflows[self.selected_workflow];

            description_lines.push(Line::from(""));
            description_lines.push(Line::from(vec![Span::styled(
                "Nodes:",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )]));
            description_lines.push(Line::from(""));

            if workflow.nodes.is_empty() {
                description_lines.push(Line::from(vec![Span::styled(
                    "  No nodes configured",
                    Style::default().fg(Color::DarkGray),
                )]));
            } else {
                for (index, node) in workflow.nodes.iter().enumerate() {
                    // Node header with ID and type
                    description_lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {}. ", index + 1),
                            Style::default().fg(Color::Yellow),
                        ),
                        Span::styled(
                            &node.id,
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" (", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            match &node.kind {
                                NodeKind::Http { .. } => "HTTP",
                                NodeKind::Shell { .. } => "Shell",
                                NodeKind::Slack { .. } => "Slack",
                                NodeKind::Log { .. } => "Log",
                            },
                            Style::default().fg(Color::Green),
                        ),
                        Span::styled(")", Style::default().fg(Color::DarkGray)),
                    ]));

                    // Node details based on type
                    match &node.kind {
                        NodeKind::Http {
                            url,
                            method,
                            expect_status,
                            ..
                        } => {
                            description_lines.push(Line::from(vec![
                                Span::styled("     Action: ", Style::default().fg(Color::DarkGray)),
                                Span::styled(method, Style::default().fg(Color::Magenta)),
                                Span::styled(" ", Style::default()),
                                Span::styled(url, Style::default().fg(Color::Blue)),
                            ]));
                            if let Some(status) = expect_status {
                                description_lines.push(Line::from(vec![
                                    Span::styled(
                                        "     Expect: ",
                                        Style::default().fg(Color::DarkGray),
                                    ),
                                    Span::styled(
                                        format!("Status {}", status),
                                        Style::default().fg(Color::Yellow),
                                    ),
                                ]));
                            }
                        }
                        NodeKind::Shell { cmd, env } => {
                            description_lines.push(Line::from(vec![
                                Span::styled(
                                    "     Command: ",
                                    Style::default().fg(Color::DarkGray),
                                ),
                                Span::styled(cmd, Style::default().fg(Color::Blue)),
                            ]));
                            if !env.is_empty() {
                                description_lines.push(Line::from(vec![
                                    Span::styled(
                                        "     Env vars: ",
                                        Style::default().fg(Color::DarkGray),
                                    ),
                                    Span::styled(
                                        env.keys()
                                            .map(|k| k.as_str())
                                            .collect::<Vec<_>>()
                                            .join(", "),
                                        Style::default().fg(Color::Yellow),
                                    ),
                                ]));
                            }
                        }
                        NodeKind::Slack { message, .. } => {
                            description_lines.push(Line::from(vec![
                                Span::styled(
                                    "     Message: ",
                                    Style::default().fg(Color::DarkGray),
                                ),
                                Span::styled(message, Style::default().fg(Color::Blue)),
                            ]));
                        }
                        NodeKind::Log { message } => {
                            description_lines.push(Line::from(vec![
                                Span::styled(
                                    "     Message: ",
                                    Style::default().fg(Color::DarkGray),
                                ),
                                Span::styled(message, Style::default().fg(Color::Blue)),
                            ]));
                        }
                    }

                    // Dependencies
                    if !node.depends_on.is_empty() {
                        description_lines.push(Line::from(vec![
                            Span::styled("     Depends on: ", Style::default().fg(Color::DarkGray)),
                            Span::styled(
                                node.depends_on.join(", "),
                                Style::default().fg(Color::Red),
                            ),
                        ]));
                    }

                    // Optional properties
                    let mut optional_props = vec![];
                    if let Some(retry) = node.retry {
                        optional_props.push(format!("retry: {}", retry));
                    }
                    if let Some(ref timeout) = node.timeout {
                        optional_props.push(format!("timeout: {}", timeout));
                    }
                    if let Some(ref when) = node.when {
                        optional_props.push(format!("when: {}", when));
                    }

                    if !optional_props.is_empty() {
                        description_lines.push(Line::from(vec![
                            Span::styled("     Options: ", Style::default().fg(Color::DarkGray)),
                            Span::styled(
                                optional_props.join(", "),
                                Style::default().fg(Color::Cyan),
                            ),
                        ]));
                    }

                    // Add spacing between nodes
                    description_lines.push(Line::from(""));
                }
            }
        }

        let description_widget = Paragraph::new(description_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Workflow Description (l to return to workflows) "),
            )
            .wrap(Wrap { trim: true })
            .scroll((self.detail_scroll, 0));

        f.render_widget(description_widget, chunks[0]);

        // Bottom help bar for description
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
