//! Terminal User Interface (TUI) module for the flowt workflow automation tool.
//!
//! This module implements a terminal-based interface for managing and monitoring workflows.
//! The TUI is organized into separate view modules:
//!
//! - `workflows_view.rs` - Main workflow management screen
//! - `logs_view.rs` - Logs display screen
//! - `description_view.rs` - Workflow description screen
//! - `help_view.rs` - Help documentation screen
//! - `utils.rs` - Shared utility functions

use crate::config::{TriggerConfig, WorkflowConfig};
use crate::engine::{Engine, SharedRuns, WorkflowRun};
use crate::storage::StorageService;
use chrono::{DateTime, Utc};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

mod description_view;
mod help_view;
mod logs_view;
mod node_detail_view;
pub mod utils;
mod workflows_view;

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
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
    pub workflows_dir: PathBuf,
    pub engine: Arc<Engine>,
    pub logs: SharedLogs,
    pub current_view: AppView,
    pub log_scroll: u16,
    pub selected_node: usize,
    pub node_detail_focused_panel: NodeDetailPanel,
    pub edit_requested: Option<String>,      // Path to file to edit
    pub service_mode: bool,                  // Whether TUI is connected to a running engine
    pub engine_takeover_requested: bool,     // Set when engine died and this TUI claimed it
    last_engine_check: std::time::Instant,   // Throttle liveness polling
}

#[derive(PartialEq)]
pub enum NodeDetailPanel {
    NodeList,
    NodeContent,
}

#[derive(PartialEq)]
pub enum AppView {
    Workflows,
    Logs,
    Description,
    Help,
    NodeDetail,
}

#[derive(PartialEq)]
pub enum FocusedPanel {
    Workflows,
    Runs,
    NodeResults,
}

impl App {
    pub fn new(
        runs: SharedRuns,
        workflows_dir: PathBuf,
        engine: Arc<Engine>,
        logs: SharedLogs,
    ) -> Self {
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
            selected_node: 0,
            node_detail_focused_panel: NodeDetailPanel::NodeList,
            edit_requested: None,
            service_mode: false,
            engine_takeover_requested: false,
            last_engine_check: std::time::Instant::now(),
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

            // Poll for events with timeout to enable automatic refresh
            if let Ok(available) = crossterm::event::poll(std::time::Duration::from_millis(500)) {
                if available {
                    if let Ok(event) = event::read() {
                        if let Event::Key(key) = event {
                            match key.code {
                                KeyCode::Char('q') => break,
                                KeyCode::Char('r') => {
                                    // Manual refresh - just continue the loop to redraw
                                    continue;
                                }
                                KeyCode::Char('l') => {
                                    // Go to logs view only from workflows
                                    if self.current_view == AppView::Workflows {
                                        self.current_view = AppView::Logs;
                                        self.detail_scroll = 0;
                                        self.log_scroll = 0;
                                    }
                                }
                                KeyCode::Char('w') => {
                                    // Go to workflows view from any view
                                    if self.current_view != AppView::Workflows {
                                        self.current_view = AppView::Workflows;
                                        self.detail_scroll = 0;
                                        self.log_scroll = 0;
                                        self.selected_node = 0;
                                        self.node_detail_focused_panel = NodeDetailPanel::NodeList;
                                    }
                                }
                                KeyCode::Char('?') => {
                                    // Show help screen
                                    self.current_view = AppView::Help;
                                    self.detail_scroll = 0;
                                }
                                KeyCode::Char('d') => {
                                    // Show description of currently selected workflow
                                    if self.current_view == AppView::Workflows {
                                        self.current_view = AppView::Description;
                                        self.detail_scroll = 0;
                                    }
                                }
                                KeyCode::Char('t') => {
                                    if self.current_view == AppView::Workflows {
                                        self.trigger_workflows();
                                    }
                                }
                                KeyCode::Char('e') => {
                                    if self.current_view == AppView::Workflows {
                                        if self.request_edit() {
                                            break; // Exit TUI for editing
                                        }
                                    }
                                }
                                KeyCode::Esc => {
                                    // Go back to workflows view from any other view
                                    if self.current_view == AppView::NodeDetail {
                                        self.current_view = AppView::Workflows;
                                        self.detail_scroll = 0;
                                        self.selected_node = 0;
                                        self.node_detail_focused_panel = NodeDetailPanel::NodeList;
                                    } else if self.current_view == AppView::Logs
                                        || self.current_view == AppView::Description
                                        || self.current_view == AppView::Help
                                    {
                                        self.current_view = AppView::Workflows;
                                        self.detail_scroll = 0;
                                        self.log_scroll = 0;
                                    }
                                }
                                KeyCode::Char(' ') | KeyCode::Enter => {
                                    if self.current_view == AppView::Workflows {
                                        if self.focused_panel == FocusedPanel::Workflows {
                                            self.toggle_workflow_enabled();
                                        } else if self.focused_panel == FocusedPanel::Runs
                                            || self.focused_panel == FocusedPanel::NodeResults
                                        {
                                            // Enter detailed view for the selected node
                                            let workflow_runs =
                                                self.get_runs_and_scheduled_for_selected_workflow();
                                            if let Some(RunOrScheduled::ActualRun(run)) =
                                                workflow_runs.get(self.selected_run)
                                            {
                                                if !run.node_results.is_empty() {
                                                    self.selected_node = 0; // Reset to first node
                                                    self.current_view = AppView::NodeDetail;
                                                    self.detail_scroll = 0;
                                                    self.node_detail_focused_panel =
                                                        NodeDetailPanel::NodeList;
                                                    // Start with node list focused
                                                }
                                            }
                                        }
                                    }
                                }
                                KeyCode::Tab | KeyCode::Right => {
                                    if self.current_view == AppView::NodeDetail {
                                        // In detail view, switch between panels
                                        self.node_detail_focused_panel =
                                            match self.node_detail_focused_panel {
                                                NodeDetailPanel::NodeList => {
                                                    NodeDetailPanel::NodeContent
                                                }
                                                NodeDetailPanel::NodeContent => {
                                                    NodeDetailPanel::NodeList
                                                }
                                            };
                                    } else {
                                        // Switch focus between panels (Tab and Right arrow)
                                        self.focused_panel = match self.focused_panel {
                                            FocusedPanel::Workflows => FocusedPanel::Runs,
                                            FocusedPanel::Runs => FocusedPanel::NodeResults,
                                            FocusedPanel::NodeResults => FocusedPanel::Workflows,
                                        };
                                        self.detail_scroll = 0; // Reset scroll when switching focus
                                    }
                                }
                                KeyCode::Left => {
                                    if self.current_view == AppView::NodeDetail {
                                        // In detail view, switch between panels
                                        self.node_detail_focused_panel =
                                            match self.node_detail_focused_panel {
                                                NodeDetailPanel::NodeList => {
                                                    NodeDetailPanel::NodeContent
                                                }
                                                NodeDetailPanel::NodeContent => {
                                                    NodeDetailPanel::NodeList
                                                }
                                            };
                                    } else {
                                        // Switch focus between panels in reverse (Left arrow)
                                        self.focused_panel = match self.focused_panel {
                                            FocusedPanel::Workflows => FocusedPanel::NodeResults,
                                            FocusedPanel::Runs => FocusedPanel::Workflows,
                                            FocusedPanel::NodeResults => FocusedPanel::Runs,
                                        };
                                        self.detail_scroll = 0; // Reset scroll when switching focus
                                    }
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if self.current_view == AppView::Logs {
                                        self.log_scroll = self.log_scroll.saturating_add(1);
                                    } else if self.current_view == AppView::Description
                                        || self.current_view == AppView::Help
                                    {
                                        self.detail_scroll = self.detail_scroll.saturating_add(1);
                                    } else if self.current_view == AppView::NodeDetail {
                                        match self.node_detail_focused_panel {
                                            NodeDetailPanel::NodeList => {
                                                let workflow_runs = self
                                                    .get_runs_and_scheduled_for_selected_workflow();
                                                if let Some(RunOrScheduled::ActualRun(run)) =
                                                    workflow_runs.get(self.selected_run)
                                                {
                                                    if self.selected_node + 1
                                                        < run.node_results.len()
                                                        && !run.node_results.is_empty()
                                                    {
                                                        self.selected_node += 1;
                                                        self.detail_scroll = 0; // Reset scroll when selecting new node
                                                    }
                                                }
                                            }
                                            NodeDetailPanel::NodeContent => {
                                                self.detail_scroll =
                                                    self.detail_scroll.saturating_add(1);
                                            }
                                        }
                                    } else {
                                        match self.focused_panel {
                                            FocusedPanel::Workflows => {
                                                let workflows = self.get_unique_workflows();
                                                if self.selected_workflow + 1 < workflows.len()
                                                    && !workflows.is_empty()
                                                {
                                                    self.selected_workflow += 1;
                                                    self.selected_run = 0; // Reset run selection
                                                    self.detail_scroll = 0;
                                                }
                                            }
                                            FocusedPanel::Runs => {
                                                let runs = self
                                                    .get_runs_and_scheduled_for_selected_workflow();
                                                if self.selected_run + 1 < runs.len()
                                                    && !runs.is_empty()
                                                {
                                                    self.selected_run += 1;
                                                    self.detail_scroll = 0;
                                                }
                                            }
                                            FocusedPanel::NodeResults => {
                                                self.detail_scroll =
                                                    self.detail_scroll.saturating_add(3);
                                            }
                                        }
                                    }
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if self.current_view == AppView::Logs {
                                        self.log_scroll = self.log_scroll.saturating_sub(1);
                                    } else if self.current_view == AppView::Description
                                        || self.current_view == AppView::Help
                                    {
                                        self.detail_scroll = self.detail_scroll.saturating_sub(1);
                                    } else if self.current_view == AppView::NodeDetail {
                                        match self.node_detail_focused_panel {
                                            NodeDetailPanel::NodeList => {
                                                if self.selected_node > 0 {
                                                    self.selected_node -= 1;
                                                    self.detail_scroll = 0; // Reset scroll when selecting new node
                                                }
                                            }
                                            NodeDetailPanel::NodeContent => {
                                                self.detail_scroll =
                                                    self.detail_scroll.saturating_sub(1);
                                            }
                                        }
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
                                                self.detail_scroll =
                                                    self.detail_scroll.saturating_sub(3);
                                            }
                                        }
                                    }
                                }
                                KeyCode::PageDown => {
                                    if self.current_view == AppView::Logs {
                                        self.log_scroll = self.log_scroll.saturating_add(10);
                                    } else if self.current_view == AppView::Description
                                        || self.current_view == AppView::Help
                                        || self.current_view == AppView::NodeDetail
                                    {
                                        self.detail_scroll = self.detail_scroll.saturating_add(10);
                                    } else if self.focused_panel == FocusedPanel::NodeResults {
                                        self.detail_scroll = self.detail_scroll.saturating_add(10);
                                    }
                                }
                                KeyCode::PageUp => {
                                    if self.current_view == AppView::Logs {
                                        self.log_scroll = self.log_scroll.saturating_sub(10);
                                    } else if self.current_view == AppView::Description
                                        || self.current_view == AppView::Help
                                        || self.current_view == AppView::NodeDetail
                                    {
                                        self.detail_scroll = self.detail_scroll.saturating_sub(10);
                                    } else if self.focused_panel == FocusedPanel::NodeResults {
                                        self.detail_scroll = self.detail_scroll.saturating_sub(10);
                                    }
                                }
                                KeyCode::Home => {
                                    if self.current_view == AppView::NodeDetail
                                        || self.current_view == AppView::Description
                                        || self.current_view == AppView::Help
                                    {
                                        self.detail_scroll = 0;
                                    } else if self.current_view == AppView::Logs {
                                        self.log_scroll = 0;
                                    }
                                }
                                KeyCode::End => {
                                    if self.current_view == AppView::NodeDetail
                                        || self.current_view == AppView::Description
                                        || self.current_view == AppView::Help
                                    {
                                        self.detail_scroll = 1000; // Large number to go to bottom
                                    } else if self.current_view == AppView::Logs {
                                        self.log_scroll = 1000;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            // If running as UI-only, periodically check whether the engine is still alive.
            // The first TUI to successfully claim the lock becomes the new engine.
            if self.service_mode
                && self.last_engine_check.elapsed() >= std::time::Duration::from_secs(2)
            {
                self.last_engine_check = std::time::Instant::now();
                if !crate::lock::is_engine_running() && crate::lock::try_claim_engine() {
                    self.engine_takeover_requested = true;
                    break;
                }
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }

    fn draw(&self, f: &mut ratatui::Frame) {
        // Add error handling for the layout
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.draw_layout(f)));

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
        // Create layout with header for service status
        let layout = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                ratatui::layout::Constraint::Length(1), // Header line
                ratatui::layout::Constraint::Min(0),    // Main content
            ])
            .split(f.size());

        // Draw service status header
        self.draw_service_status_header(f, layout[0]);

        // Draw the main view in the remaining area
        match self.current_view {
            AppView::Workflows => self.draw_workflows_view_with_area(f, layout[1]),
            AppView::Logs => self.draw_logs_view_with_area(f, layout[1]),
            AppView::Description => self.draw_description_view_with_area(f, layout[1]),
            AppView::Help => self.draw_help_view_with_area(f, layout[1]),
            AppView::NodeDetail => self.draw_node_detail_view_with_area(f, layout[1]),
        }
    }

    fn draw_service_status_header(&self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        use ratatui::style::{Color, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::Paragraph;

        let status_line = if self.service_mode {
            Line::from(vec![
                Span::styled("🔗 ", Style::default().fg(Color::Green)),
                Span::styled(
                    "Connected to Flowt service",
                    Style::default().fg(Color::Green),
                ),
                Span::styled(
                    " • Jobs running by other instances",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled("⚡ ", Style::default().fg(Color::Cyan)),
                Span::styled("TUI Engine Mode", Style::default().fg(Color::Cyan)),
                Span::styled(
                    " • Jobs active in this session",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        };

        let header = Paragraph::new(vec![status_line]);
        f.render_widget(header, area);
    }

    pub fn log_info(&self, workflow_name: &str, message: String) {
        let entry = LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            message,
        };

        // Add to in-memory logs
        if let Ok(mut logs) = self.logs.try_lock() {
            let workflow_logs = logs
                .entry(workflow_name.to_string())
                .or_insert_with(Vec::new);
            workflow_logs.push(entry.clone());
            // Keep only the last 1000 log entries per workflow
            if workflow_logs.len() > 1000 {
                let excess = workflow_logs.len() - 1000;
                workflow_logs.drain(0..excess);
            }
        }

        // Also persist to database
        if let Ok(storage) = StorageService::new() {
            let _ = storage.save_log_entry(workflow_name, &entry);
        }
    }

    pub fn log_warning(&self, workflow_name: &str, message: String) {
        let entry = LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Warning,
            message,
        };

        // Add to in-memory logs
        if let Ok(mut logs) = self.logs.try_lock() {
            let workflow_logs = logs
                .entry(workflow_name.to_string())
                .or_insert_with(Vec::new);
            workflow_logs.push(entry.clone());
            if workflow_logs.len() > 1000 {
                let excess = workflow_logs.len() - 1000;
                workflow_logs.drain(0..excess);
            }
        }

        // Also persist to database
        if let Ok(storage) = StorageService::new() {
            let _ = storage.save_log_entry(workflow_name, &entry);
        }
    }

    pub fn log_error(&self, workflow_name: &str, message: String) {
        let entry = LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Error,
            message,
        };

        // Add to in-memory logs
        if let Ok(mut logs) = self.logs.try_lock() {
            let workflow_logs = logs
                .entry(workflow_name.to_string())
                .or_insert_with(Vec::new);
            workflow_logs.push(entry.clone());
            if workflow_logs.len() > 1000 {
                let excess = workflow_logs.len() - 1000;
                workflow_logs.drain(0..excess);
            }
        }

        // Also persist to database
        if let Ok(storage) = StorageService::new() {
            let _ = storage.save_log_entry(workflow_name, &entry);
        }
    }

    fn request_edit(&mut self) -> bool {
        let workflows = self.get_unique_workflows();
        if workflows.is_empty() || self.selected_workflow >= workflows.len() {
            self.log_warning("System", "No workflow selected to edit".to_string());
            return false;
        }

        let selected_workflow = &workflows[self.selected_workflow];
        let workflow_name = &selected_workflow.name;

        // Find the YAML file for this workflow
        if let Ok(entries) = std::fs::read_dir(&self.workflows_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let extension = path.extension().and_then(|e| e.to_str());
                if extension == Some("yaml") || extension == Some("yml") {
                    if let Ok(workflow) = WorkflowConfig::load(&path) {
                        if workflow.name == *workflow_name {
                            self.edit_requested = Some(path.to_string_lossy().to_string());
                            return true; // Signal to exit TUI
                        }
                    }
                }
            }
        }

        self.log_error(
            "System",
            format!("Could not find YAML file for workflow: {}", workflow_name),
        );
        false
    }

    pub fn edit_workflow_external(file_path: &str) {
        // Get editor from environment variable or use fallbacks
        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_else(|_| {
                // Try common editors in order of preference
                for editor in ["vim", "nano", "code", "emacs"] {
                    if std::process::Command::new("which")
                        .arg(editor)
                        .output()
                        .map(|output| output.status.success())
                        .unwrap_or(false)
                    {
                        return editor.to_string();
                    }
                }
                "vi".to_string() // Last resort fallback
            });

        println!("Opening {} for editing with {}", file_path, editor);

        // Launch editor
        let result = std::process::Command::new(&editor).arg(file_path).status();

        match result {
            Ok(status) => {
                if status.success() {
                    println!("Finished editing {}", file_path);
                } else {
                    println!("Editor exited with non-zero status: {}", status);
                }
            }
            Err(e) => {
                println!("Failed to launch editor {}: {}", editor, e);
            }
        }
    }

    fn trigger_workflows(&self) {
        if let Ok(workflows) =
            WorkflowConfig::load_all(&self.workflows_dir, Some(self.logs.clone()))
        {
            if workflows.is_empty() || self.selected_workflow >= workflows.len() {
                self.log_info("System", "No workflow selected or available".to_string());
                return;
            }

            let selected_workflow = &workflows[self.selected_workflow];

            if !selected_workflow.enabled {
                self.log_info(
                    "System",
                    format!("Workflow '{}' is disabled", selected_workflow.name),
                );
                return;
            }

            let workflow_name = selected_workflow.name.clone();
            self.log_info(
                &workflow_name,
                format!("Manually triggered workflow: {}", workflow_name),
            );

            let wf = selected_workflow.clone();
            let engine_clone = self.engine.clone();
            let logs_clone = self.logs.clone();
            tokio::spawn(async move {
                match engine_clone.run_workflow(&wf).await {
                    Ok(run) => {
                        if let Ok(mut logs) = logs_clone.try_lock() {
                            match run.status {
                                crate::engine::RunStatus::Success => {
                                    let workflow_logs =
                                        logs.entry(workflow_name.clone()).or_insert_with(Vec::new);
                                    workflow_logs.push(crate::tui::LogEntry {
                                        timestamp: chrono::Utc::now(),
                                        level: crate::tui::LogLevel::Info,
                                        message: format!(
                                            "Manual workflow completed successfully: {}",
                                            workflow_name
                                        ),
                                    });
                                }
                                crate::engine::RunStatus::Failed => {
                                    let workflow_logs =
                                        logs.entry(workflow_name.clone()).or_insert_with(Vec::new);
                                    workflow_logs.push(crate::tui::LogEntry {
                                        timestamp: chrono::Utc::now(),
                                        level: crate::tui::LogLevel::Error,
                                        message: format!(
                                            "✗ Manual workflow failed: {}",
                                            workflow_name
                                        ),
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        if let Ok(mut logs) = logs_clone.try_lock() {
                            let workflow_logs =
                                logs.entry(workflow_name.clone()).or_insert_with(Vec::new);
                            workflow_logs.push(crate::tui::LogEntry {
                                timestamp: chrono::Utc::now(),
                                level: crate::tui::LogLevel::Error,
                                message: format!(
                                    "✗ Error running manual workflow {}: {}",
                                    workflow_name, e
                                ),
                            });
                        }
                    }
                }
            });
        } else {
            self.log_info(
                "System",
                "Failed to load workflows for triggering".to_string(),
            );
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
                    if let Ok(mut workflow) = WorkflowConfig::load(&path) {
                        if workflow.name == *workflow_name {
                            workflow.toggle_enabled();
                            match workflow.save(&path) {
                                Ok(_) => {
                                    let new_state = if current_enabled {
                                        "disabled"
                                    } else {
                                        "enabled"
                                    };
                                    self.log_info(
                                        workflow_name,
                                        format!("Workflow {} {}", workflow_name, new_state),
                                    );
                                }
                                Err(e) => {
                                    self.log_error(
                                        workflow_name,
                                        format!("Failed to save workflow {}: {}", workflow_name, e),
                                    );
                                }
                            }
                            break;
                        }
                    }
                }
            }
        } else {
            self.log_error(
                "System",
                format!(
                    "Failed to access workflows directory: {}",
                    self.workflows_dir.display()
                ),
            );
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
        if let Ok(configs) = WorkflowConfig::load_all(&self.workflows_dir, Some(self.logs.clone()))
        {
            if workflows.is_empty() && !configs.is_empty() {
                self.log_info(
                    "System",
                    format!(
                        "Loaded {} workflows from {}",
                        configs.len(),
                        self.workflows_dir.display()
                    ),
                );
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
        result.sort_by(|a, b| match (a, b) {
            (RunOrScheduled::ScheduledRun { .. }, RunOrScheduled::ActualRun(_)) => {
                std::cmp::Ordering::Less
            }
            (RunOrScheduled::ActualRun(_), RunOrScheduled::ScheduledRun { .. }) => {
                std::cmp::Ordering::Greater
            }
            (RunOrScheduled::ActualRun(a), RunOrScheduled::ActualRun(b)) => {
                b.started_at.cmp(&a.started_at)
            }
            (RunOrScheduled::ScheduledRun { .. }, RunOrScheduled::ScheduledRun { .. }) => {
                std::cmp::Ordering::Equal
            }
        });

        result
    }
}
