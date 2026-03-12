use crate::config::TriggerConfig;
use crate::engine::RunStatus;
use crate::tui::{utils::format_duration, App, FocusedPanel, RunOrScheduled};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

impl App {
    pub fn draw_workflows_view(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(35),
                Constraint::Percentage(40),
            ])
            .split(f.size());

        // First panel: unique workflow names
        let workflows = self.get_unique_workflows();
        let workflow_items: Vec<ListItem> = workflows
            .iter()
            .map(|config| {
                let trigger_type = if !config.triggers.is_empty() {
                    match &config.triggers[0] {
                        TriggerConfig::Manual => "manual",
                        TriggerConfig::Cron { .. } => "cron",
                    }
                } else {
                    "none"
                };

                let status_icon = if config.enabled { "●" } else { "○" };
                let status_color = if config.enabled {
                    Color::Green
                } else {
                    Color::DarkGray
                };

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{} ", status_icon),
                        Style::default().fg(status_color),
                    ),
                    Span::styled(config.name.clone(), Style::default().fg(Color::White)),
                    Span::styled(
                        format!(" ({})", trigger_type),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect();

        let mut workflow_state = ListState::default();
        if !workflows.is_empty() && self.selected_workflow < workflows.len() {
            workflow_state.select(Some(self.selected_workflow));
        }

        let workflow_list = List::new(workflow_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Workflows ")
                    .border_style(if self.focused_panel == FocusedPanel::Workflows {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_stateful_widget(workflow_list, chunks[0], &mut workflow_state);

        // Second panel: runs for selected workflow
        let workflow_runs = self.get_runs_and_scheduled_for_selected_workflow();
        let run_items: Vec<ListItem> = workflow_runs
            .iter()
            .map(|run_or_scheduled| match run_or_scheduled {
                RunOrScheduled::ActualRun(run) => {
                    let (icon, color) = match &run.status {
                        RunStatus::Running => ("⟳", Color::Yellow),
                        RunStatus::Success => ("✔", Color::Green),
                        RunStatus::Failed => ("✘", Color::Red),
                    };
                    let timestamp = run
                        .started_at
                        .with_timezone(&chrono::Local)
                        .format("%m/%d %H:%M:%S")
                        .to_string();

                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{} ", icon), Style::default().fg(color)),
                        Span::styled(format!("#{} ", run.id), Style::default().fg(Color::White)),
                        Span::styled(timestamp, Style::default().fg(Color::White)),
                    ]))
                }
                RunOrScheduled::ScheduledRun { next_run, .. } => {
                    let timestamp = next_run
                        .with_timezone(&chrono::Local)
                        .format("%m/%d %H:%M:%S")
                        .to_string();

                    ListItem::new(Line::from(vec![
                        Span::styled("🕐 ", Style::default().fg(Color::Cyan)),
                        Span::styled("next ", Style::default().fg(Color::Cyan)),
                        Span::styled(timestamp, Style::default().fg(Color::White)),
                    ]))
                }
            })
            .collect();

        let mut run_state = ListState::default();
        if !workflow_runs.is_empty() && self.selected_run < workflow_runs.len() {
            run_state.select(Some(self.selected_run));
        }

        let run_list = List::new(run_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Runs ")
                    .border_style(if self.focused_panel == FocusedPanel::Runs {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_stateful_widget(run_list, chunks[1], &mut run_state);

        // Third panel: selected run details
        let detail_text = if workflows.is_empty() {
            vec![
                Line::from(""),
                Line::from("No workflows found."),
                Line::from(""),
                Line::from("Create a workflow YAML file in the"),
                Line::from(format!("{} directory.", self.workflows_dir)),
                Line::from(""),
                Line::from("Press 't' to refresh and trigger workflows."),
            ]
        } else if workflow_runs.is_empty() {
            vec![
                Line::from(""),
                Line::from("No runs for this workflow yet."),
                Line::from(""),
                Line::from("Press 't' to trigger the workflow."),
            ]
        } else if let Some(run_or_scheduled) = workflow_runs.get(self.selected_run) {
            match run_or_scheduled {
                RunOrScheduled::ActualRun(run) => crate::tui::utils::run_details(run),
                RunOrScheduled::ScheduledRun {
                    workflow_name,
                    next_run,
                } => {
                    vec![
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("Workflow: ", Style::default().fg(Color::DarkGray)),
                            Span::styled(
                                workflow_name.clone(),
                                Style::default()
                                    .fg(Color::White)
                                    .add_modifier(Modifier::BOLD),
                            ),
                        ]),
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
                            Span::styled("Scheduled", Style::default().fg(Color::Cyan)),
                        ]),
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("Next Run: ", Style::default().fg(Color::DarkGray)),
                            Span::styled(
                                next_run
                                    .with_timezone(&chrono::Local)
                                    .format("%Y-%m-%d %H:%M:%S")
                                    .to_string(),
                                Style::default().fg(Color::White),
                            ),
                        ]),
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("In: ", Style::default().fg(Color::DarkGray)),
                            Span::styled(
                                format_duration(*next_run - chrono::Utc::now()),
                                Style::default().fg(Color::Cyan),
                            ),
                        ]),
                        Line::from(""),
                        Line::from(
                            "This run will be triggered automatically by the cron scheduler.",
                        ),
                    ]
                }
            }
        } else {
            vec![]
        };

        let detail = Paragraph::new(detail_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Results ")
                    .border_style(if self.focused_panel == FocusedPanel::NodeResults {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }),
            )
            .wrap(Wrap { trim: true })
            .scroll((self.detail_scroll, 0));

        f.render_widget(detail, chunks[2]);

        // Bottom help bar - responsive based on terminal width
        let area = f.size();

        // Define help items in order of priority (most important first)

        let help_items = if area.width >= 130 {
            // Full help bar for wide terminals
            vec![
                " ↑/↓ navigate/scroll ",
                "| Tab/←/→ switch panel ",
                "| Space toggle enable ",
                "| Enter details ",
                "| r refresh ",
                "| l logs ",
                "| d describe ",
                "| t trigger ",
                "| ? help ",
                "| q quit ",
            ]
        } else if area.width >= 120 {
            // Full help bar for wide terminals
            vec![
                " ↑/↓ navigate/scroll ",
                "| Tab/←/→ switch panel ",
                "| Space toggle enable ",
                "| r refresh ",
                "| l logs ",
                "| d describe ",
                "| t trigger ",
                "| ? help ",
                "| q quit ",
            ]
        } else if area.width >= 90 {
            // Medium help bar - remove less critical items
            vec![
                " ↑/↓ nav ",
                "| Tab/←/→ panel ",
                "| Space toggle ",
                "| Enter details ",
                "| r refresh ",
                "| l logs ",
                "| t trigger ",
                "| ? help ",
                "| q quit ",
            ]
        } else if area.width >= 60 {
            // Compact help bar
            vec![
                " ↑/↓ nav ",
                "| ←/→ panel ",
                "| Enter details ",
                "| ? help ",
                "| q quit ",
            ]
        } else {
            // Minimal help bar for very narrow terminals
            vec![" ↑/↓ ", "| ←/→ ", "| ? help ", "| q quit "]
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
}
