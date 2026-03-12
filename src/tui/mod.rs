use crate::engine::{Engine, NodeStatus, RunStatus, SharedRuns, WorkflowRun};
use crate::config::{WorkflowConfig, TriggerConfig};
use std::str::FromStr;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub enum RunOrScheduled {
    ActualRun(WorkflowRun),
    ScheduledRun {
        workflow_name: String,
        next_run: DateTime<Utc>,
    },
}

pub type SharedLogs = Arc<Mutex<HashMap<String, Vec<LogEntry>>>>;

pub struct App {
    pub runs: SharedRuns,
    pub selected_workflow: usize,
    pub selected_run: usize,
    pub detail_scroll: u16,
    pub focused_panel: FocusedPanel,
    pub workflows_dir: String,
    pub engine: Arc<Engine>,
    pub logs: SharedLogs,
    pub current_view: AppView,
    pub log_scroll: u16,
}

#[derive(PartialEq)]
pub enum AppView {
    Workflows,
    Logs,
}

#[derive(PartialEq)]
pub enum FocusedPanel {
    Workflows,
    Runs,
    NodeResults,
}

impl App {
    pub fn new(runs: SharedRuns, workflows_dir: String, engine: Arc<Engine>, logs: SharedLogs) -> Self {
        Self {
            runs,
            selected_workflow: 0,
            selected_run: 0,
            detail_scroll: 0,
            focused_panel: FocusedPanel::Workflows,
            workflows_dir,
            engine,
            logs,
            current_view: AppView::Workflows,
            log_scroll: 0,
        }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        loop {
            terminal.draw(|f| self.draw(f))?;

            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('l') => {
                            // Toggle between workflows and logs view
                            self.current_view = match self.current_view {
                                AppView::Workflows => AppView::Logs,
                                AppView::Logs => AppView::Workflows,
                            };
                            self.detail_scroll = 0;
                            self.log_scroll = 0;
                        }
                        KeyCode::Char('t') => {
                            if self.current_view == AppView::Workflows {
                                self.trigger_workflows();
                            }
                        }
                        KeyCode::Char(' ') | KeyCode::Enter => {
                            if self.focused_panel == FocusedPanel::Workflows {
                                self.toggle_workflow_enabled();
                            }
                        }
                        KeyCode::Tab => {
                            // Switch focus between panels
                            self.focused_panel = match self.focused_panel {
                                FocusedPanel::Workflows => FocusedPanel::Runs,
                                FocusedPanel::Runs => FocusedPanel::NodeResults,
                                FocusedPanel::NodeResults => FocusedPanel::Workflows,
                            };
                            self.detail_scroll = 0; // Reset scroll when switching focus
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if self.current_view == AppView::Logs {
                                self.log_scroll = self.log_scroll.saturating_add(1);
                            } else {
                                match self.focused_panel {
                                    FocusedPanel::Workflows => {
                                        let workflows = self.get_unique_workflows();
                                        if self.selected_workflow + 1 < workflows.len() && !workflows.is_empty() {
                                            self.selected_workflow += 1;
                                            self.selected_run = 0; // Reset run selection
                                            self.detail_scroll = 0;
                                        }
                                    }
                                    FocusedPanel::Runs => {
                                        let runs = self.get_runs_and_scheduled_for_selected_workflow();
                                        if self.selected_run + 1 < runs.len() && !runs.is_empty() {
                                            self.selected_run += 1;
                                            self.detail_scroll = 0;
                                        }
                                    }
                                    FocusedPanel::NodeResults => {
                                        self.detail_scroll = self.detail_scroll.saturating_add(3);
                                    }
                                }
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if self.current_view == AppView::Logs {
                                self.log_scroll = self.log_scroll.saturating_sub(1);
                            } else {
                                match self.focused_panel {
                                    FocusedPanel::Workflows => {
                                        if self.selected_workflow > 0 {
                                            self.selected_workflow -= 1;
                                            self.selected_run = 0; // Reset run selection
                                            self.detail_scroll = 0;
                                        }
                                    }
                                    FocusedPanel::Runs => {
                                        if self.selected_run > 0 {
                                            self.selected_run -= 1;
                                            self.detail_scroll = 0;
                                        }
                                    }
                                    FocusedPanel::NodeResults => {
                                        self.detail_scroll = self.detail_scroll.saturating_sub(3);
                                    }
                                }
                            }
                        }
                        KeyCode::PageDown => {
                            if self.current_view == AppView::Logs {
                                self.log_scroll = self.log_scroll.saturating_add(10);
                            } else if self.focused_panel == FocusedPanel::NodeResults {
                                self.detail_scroll = self.detail_scroll.saturating_add(10);
                            }
                        }
                        KeyCode::PageUp => {
                            if self.current_view == AppView::Logs {
                                self.log_scroll = self.log_scroll.saturating_sub(10);
                            } else if self.focused_panel == FocusedPanel::NodeResults {
                                self.detail_scroll = self.detail_scroll.saturating_sub(10);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }

    fn draw(&self, f: &mut ratatui::Frame) {
        // Add error handling for the layout
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.draw_layout(f)
        }));
        
        if result.is_err() {
            // Fallback simple display on error
            let error_text = vec![
                Line::from(""),
                Line::from("Error rendering TUI layout."),
                Line::from(""),
                Line::from("Press 'q' to quit and restart."),
            ];
            let error_widget = Paragraph::new(error_text)
                .block(Block::default().borders(Borders::ALL).title(" Error "));
            f.render_widget(error_widget, f.size());
        }
    }

    fn draw_layout(&self, f: &mut ratatui::Frame) {
        match self.current_view {
            AppView::Workflows => self.draw_workflows_view(f),
            AppView::Logs => self.draw_logs_view(f),
        }
    }

    fn draw_workflows_view(&self, f: &mut ratatui::Frame) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(25), Constraint::Percentage(35), Constraint::Percentage(40)])
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
                        TriggerConfig::Webhook { .. } => "webhook",
                    }
                } else {
                    "none"
                };
                
                let status_icon = if config.enabled { "●" } else { "○" };
                let status_color = if config.enabled { Color::Green } else { Color::DarkGray };
                
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", status_icon), Style::default().fg(status_color)),
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
            .map(|run_or_scheduled| {
                match run_or_scheduled {
                    RunOrScheduled::ActualRun(run) => {
                        let (icon, color) = match &run.status {
                            RunStatus::Running => ("⟳", Color::Yellow),
                            RunStatus::Success => ("✔", Color::Green),
                            RunStatus::Failed => ("✘", Color::Red),
                        };
                        let timestamp = run.started_at
                            .with_timezone(&chrono::Local)
                            .format("%m/%d %H:%M:%S")
                            .to_string();
                        
                        ListItem::new(Line::from(vec![
                            Span::styled(format!("{} ", icon), Style::default().fg(color)),
                            Span::styled(
                                format!("#{} ", run.id),
                                Style::default().fg(Color::White),
                            ),
                            Span::styled(
                                timestamp,
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]))
                    },
                    RunOrScheduled::ScheduledRun { next_run, .. } => {
                        let timestamp = next_run
                            .with_timezone(&chrono::Local)
                            .format("%m/%d %H:%M:%S")
                            .to_string();
                        
                        ListItem::new(Line::from(vec![
                            Span::styled("🕐 ", Style::default().fg(Color::Cyan)),
                            Span::styled(
                                "next ",
                                Style::default().fg(Color::Cyan),
                            ),
                            Span::styled(
                                timestamp,
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]))
                    }
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
                RunOrScheduled::ActualRun(run) => run_details(run),
                RunOrScheduled::ScheduledRun { workflow_name, next_run } => {
                    vec![
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("Workflow: ", Style::default().fg(Color::DarkGray)),
                            Span::styled(workflow_name.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
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
                                next_run.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string(),
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
                        Line::from("This run will be triggered automatically by the cron scheduler."),
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

        // Bottom help bar
        let help = Paragraph::new(Line::from(vec![
            Span::styled(" ↑/↓ navigate/scroll ", Style::default().fg(Color::DarkGray)),
            Span::styled("| Tab switch panel ", Style::default().fg(Color::DarkGray)),
            Span::styled("| Space toggle enable ", Style::default().fg(Color::DarkGray)),
            Span::styled("| l logs ", Style::default().fg(Color::DarkGray)),
            Span::styled("| t trigger ", Style::default().fg(Color::DarkGray)),
            Span::styled("| q quit ", Style::default().fg(Color::DarkGray)),
        ]));
        let area = f.size();
        let help_area = ratatui::layout::Rect {
            x: 0,
            y: area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        f.render_widget(help, help_area);
    }

    fn draw_logs_view(&self, f: &mut ratatui::Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(f.size());

        // Get the currently selected workflow name
        let workflows = self.get_unique_workflows();
        let selected_workflow_name = if !workflows.is_empty() && self.selected_workflow < workflows.len() {
            &workflows[self.selected_workflow].name
        } else {
            "System"
        };

        // Main logs area - show only logs for selected workflow
        let logs = self.logs.lock().unwrap();
        let workflow_logs = logs.get(selected_workflow_name).cloned().unwrap_or_default();
        
        let log_items: Vec<Line> = workflow_logs
            .iter()
            .map(|log| {
                let timestamp = log.timestamp
                    .with_timezone(&chrono::Local)
                    .format("%m/%d %H:%M:%S")
                    .to_string();
                
                let (icon, color) = match log.level {
                    LogLevel::Info => ("ℹ", Color::Blue),
                    LogLevel::Warning => ("⚠", Color::Yellow),
                    LogLevel::Error => ("✘", Color::Red),
                };

                Line::from(vec![
                    Span::styled(format!("{} ", timestamp), Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("{} ", icon), Style::default().fg(color)),
                    Span::styled(log.message.clone(), Style::default().fg(Color::White)),
                ])
            })
            .collect();

        let logs_widget = Paragraph::new(log_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Logs: {} (l to return to workflows) ", selected_workflow_name)),
            )
            .scroll((self.log_scroll, 0));

        f.render_widget(logs_widget, chunks[0]);

        // Bottom help bar for logs
        let help = Paragraph::new(Line::from(vec![
            Span::styled(" ↑/↓ scroll ", Style::default().fg(Color::DarkGray)),
            Span::styled("| PgUp/PgDn fast scroll ", Style::default().fg(Color::DarkGray)),
            Span::styled("| f back to workflows ", Style::default().fg(Color::DarkGray)),
            Span::styled("| q quit ", Style::default().fg(Color::DarkGray)),
        ]));
        f.render_widget(help, chunks[1]);
    }

    pub fn log_info(&self, workflow_name: &str, message: String) {
        let entry = LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            message,
        };
        if let Ok(mut logs) = self.logs.try_lock() {
            let workflow_logs = logs.entry(workflow_name.to_string()).or_insert_with(Vec::new);
            workflow_logs.push(entry);
            // Keep only the last 1000 log entries per workflow
            if workflow_logs.len() > 1000 {
                let excess = workflow_logs.len() - 1000;
                workflow_logs.drain(0..excess);
            }
        }
    }

    pub fn log_warning(&self, workflow_name: &str, message: String) {
        let entry = LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Warning,
            message,
        };
        if let Ok(mut logs) = self.logs.try_lock() {
            let workflow_logs = logs.entry(workflow_name.to_string()).or_insert_with(Vec::new);
            workflow_logs.push(entry);
            if workflow_logs.len() > 1000 {
                let excess = workflow_logs.len() - 1000;
                workflow_logs.drain(0..excess);
            }
        }
    }

    pub fn log_error(&self, workflow_name: &str, message: String) {
        let entry = LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Error,
            message,
        };
        if let Ok(mut logs) = self.logs.try_lock() {
            let workflow_logs = logs.entry(workflow_name.to_string()).or_insert_with(Vec::new);
            workflow_logs.push(entry);
            if workflow_logs.len() > 1000 {
                let excess = workflow_logs.len() - 1000;
                workflow_logs.drain(0..excess);
            }
        }
    }

    fn trigger_workflows(&self) {
        if let Ok(workflows) = WorkflowConfig::load_all(&self.workflows_dir, Some(self.logs.clone())) {
            let enabled_count = workflows.iter().filter(|w| w.enabled).count();
            self.log_info("System", format!("Triggering {} enabled workflows", enabled_count));
            
            // Run all enabled workflows
            for wf in workflows {
                if wf.enabled {
                    let workflow_name = wf.name.clone();
                    self.log_info(&workflow_name, format!("Starting workflow: {}", workflow_name));
                    let engine_clone = self.engine.clone();
                    let logs_clone = self.logs.clone();
                    tokio::spawn(async move {
                        match engine_clone.run_workflow(&wf).await {
                            Ok(run) => {
                                if let Ok(mut logs) = logs_clone.try_lock() {
                                    match run.status {
                                        crate::engine::RunStatus::Success => {
                                            let workflow_logs = logs.entry(workflow_name.clone()).or_insert_with(Vec::new);
                                            workflow_logs.push(crate::tui::LogEntry {
                                                timestamp: chrono::Utc::now(),
                                                level: crate::tui::LogLevel::Info,
                                                message: format!("✓ Workflow completed successfully: {}", workflow_name),
                                            });
                                        },
                                        crate::engine::RunStatus::Failed => {
                                            let workflow_logs = logs.entry(workflow_name.clone()).or_insert_with(Vec::new);
                                            workflow_logs.push(crate::tui::LogEntry {
                                                timestamp: chrono::Utc::now(),
                                                level: crate::tui::LogLevel::Error,
                                                message: format!("✗ Workflow failed: {}", workflow_name),
                                            });
                                        },
                                        _ => {}
                                    }
                                }
                            },
                            Err(e) => {
                                if let Ok(mut logs) = logs_clone.try_lock() {
                                    let workflow_logs = logs.entry(workflow_name.clone()).or_insert_with(Vec::new);
                                    workflow_logs.push(crate::tui::LogEntry {
                                        timestamp: chrono::Utc::now(),
                                        level: crate::tui::LogLevel::Error,
                                        message: format!("✗ Error running workflow {}: {}", workflow_name, e),
                                    });
                                }
                            }
                        }
                    });
                }
            }
        } else {
            self.log_error("System", "Failed to load workflows for triggering".to_string());
        }
    }

    fn toggle_workflow_enabled(&mut self) {
        let workflows = self.get_unique_workflows();
        if workflows.is_empty() || self.selected_workflow >= workflows.len() {
            self.log_warning("System", "No workflow selected to toggle".to_string());
            return;
        }

        let workflow_name = &workflows[self.selected_workflow].name;
        let current_enabled = workflows[self.selected_workflow].enabled;
        
        // Find and toggle the workflow in the filesystem
        if let Ok(entries) = std::fs::read_dir(&self.workflows_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let extension = path.extension().and_then(|e| e.to_str());
                if extension == Some("yaml") || extension == Some("yml") {
                    if let Ok(mut workflow) = WorkflowConfig::load(path.to_str().unwrap_or("")) {
                        if workflow.name == *workflow_name {
                            workflow.toggle_enabled();
                            match workflow.save(path.to_str().unwrap_or("")) {
                                Ok(_) => {
                                    let new_state = if current_enabled { "disabled" } else { "enabled" };
                                    self.log_info(workflow_name, format!("Workflow {} {}", workflow_name, new_state));
                                },
                                Err(e) => {
                                    self.log_error(workflow_name, format!("Failed to save workflow {}: {}", workflow_name, e));
                                }
                            }
                            break;
                        }
                    }
                }
            }
        } else {
            self.log_error("System", format!("Failed to access workflows directory: {}", self.workflows_dir));
        }
    }

    fn get_unique_workflows(&self) -> Vec<WorkflowConfig> {
        let mut workflows: HashSet<String> = HashSet::new();
        let mut workflow_configs: Vec<WorkflowConfig> = vec![];
        
        // Add workflows from existing runs
        let runs = self.runs.lock().unwrap();
        for run in runs.iter() {
            workflows.insert(run.workflow_name.clone());
        }
        
        // Also add workflows from filesystem (even if not run yet)
        if let Ok(configs) = WorkflowConfig::load_all(&self.workflows_dir, Some(self.logs.clone())) {
            if workflows.is_empty() && !configs.is_empty() {
                self.log_info("System", format!("Loaded {} workflows from {}", configs.len(), self.workflows_dir));
            }
            for config in configs {
                if !workflows.contains(&config.name) {
                    workflows.insert(config.name.clone());
                }
                workflow_configs.push(config);
            }
        }
        
        // Sort by name
        workflow_configs.sort_by(|a, b| a.name.cmp(&b.name));
        workflow_configs
    }

    fn get_runs_and_scheduled_for_selected_workflow(&self) -> Vec<RunOrScheduled> {
        let workflows = self.get_unique_workflows();
        
        if workflows.is_empty() || self.selected_workflow >= workflows.len() {
            return vec![];
        }
        
        let selected_workflow = &workflows[self.selected_workflow];
        let mut result = Vec::new();
        
        // Add actual runs
        let runs = self.runs.lock().unwrap();
        for run in runs.iter() {
            if &run.workflow_name == &selected_workflow.name {
                result.push(RunOrScheduled::ActualRun(run.clone()));
            }
        }
        
        // Add scheduled run for cron workflows
        for trigger in &selected_workflow.triggers {
            if let TriggerConfig::Cron { schedule } = trigger {
                if let Ok(cron_schedule) = cron::Schedule::from_str(schedule) {
                    if let Some(next_run) = cron_schedule.upcoming(chrono::Utc).take(1).next() {
                        result.push(RunOrScheduled::ScheduledRun {
                            workflow_name: selected_workflow.name.clone(),
                            next_run,
                        });
                        break; // Only show one next scheduled run
                    }
                }
            }
        }
        
        // Sort: scheduled runs first, then actual runs by start time (newest first)
        result.sort_by(|a, b| {
            match (a, b) {
                (RunOrScheduled::ScheduledRun { .. }, RunOrScheduled::ActualRun(_)) => std::cmp::Ordering::Less,
                (RunOrScheduled::ActualRun(_), RunOrScheduled::ScheduledRun { .. }) => std::cmp::Ordering::Greater,
                (RunOrScheduled::ActualRun(a), RunOrScheduled::ActualRun(b)) => b.started_at.cmp(&a.started_at),
                (RunOrScheduled::ScheduledRun { .. }, RunOrScheduled::ScheduledRun { .. }) => std::cmp::Ordering::Equal,
            }
        });
        
        result
    }
}

fn run_details(run: &WorkflowRun) -> Vec<Line<'static>> {
    let mut lines = vec![];

    lines.push(Line::from(vec![
        Span::styled("Workflow: ", Style::default().fg(Color::DarkGray)),
        Span::styled(run.workflow_name.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Run ID:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("#{}", run.id), Style::default().fg(Color::Cyan)),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Started:  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            run.started_at.format("%H:%M:%S").to_string(),
            Style::default().fg(Color::White),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Nodes:", Style::default().fg(Color::DarkGray))));
    lines.push(Line::from(""));

    for result in &run.node_results {
        let (icon, color) = match &result.status {
            NodeStatus::Pending => ("○", Color::DarkGray),
            NodeStatus::Running => ("⟳", Color::Yellow),
            NodeStatus::Success => ("✔", Color::Green),
            NodeStatus::Failed(_) => ("✘", Color::Red),
            NodeStatus::Skipped => ("-", Color::DarkGray),
        };

        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", icon), Style::default().fg(color)),
            Span::styled(result.node_id.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]));

        if !result.output.is_empty() {
            for line in result.output.lines().take(3) {
                lines.push(Line::from(vec![Span::styled(
                    format!("      {}", line),
                    Style::default().fg(Color::DarkGray),
                )]));
            }
        }

        if let NodeStatus::Failed(err) = &result.status {
            lines.push(Line::from(vec![Span::styled(
                format!("      Error: {}", err),
                Style::default().fg(Color::Red),
            )]));
        }

        lines.push(Line::from(""));
    }

    lines
}

fn format_duration(duration: chrono::Duration) -> String {
    let total_seconds = duration.num_seconds();
    
    if total_seconds < 0 {
        return "overdue".to_string();
    }
    
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    
    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}
