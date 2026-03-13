mod config;
mod engine;
mod lock;
mod storage;
mod tui;

use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use config::{TriggerConfig, WorkflowConfig};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use engine::Engine;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use storage::StorageService;
use tui::{LogEntry, LogLevel, SharedLogs};

fn default_workflows_dir() -> String {
    std::env::var("FLOWT_DIR")
        .map(|dir| format!("{}/workflows", dir))
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{}/.flowt/workflows", home)
        })
}

// Helper function to log with persistence
fn log_to_persistent_storage(
    logs: &SharedLogs,
    workflow_name: &str,
    level: LogLevel,
    message: &str,
) {
    let entry = LogEntry {
        timestamp: chrono::Utc::now(),
        level,
        message: message.to_string(),
    };

    // Add to in-memory logs
    if let Ok(mut logs_guard) = logs.try_lock() {
        logs_guard
            .entry(workflow_name.to_string())
            .or_insert_with(Vec::new)
            .push(entry.clone());
    }

    // Also persist to database
    if let Ok(history) = StorageService::new() {
        let _ = history.save_log_entry(workflow_name, &entry);
    }
}

// Loads historical logs from database into memory
fn load_historical_logs(logs: &SharedLogs) {
    if let Ok(history) = StorageService::new() {
        if let Ok(recent_runs) = history.get_recent_workflow_runs(Some(50)) {
            let workflow_names: std::collections::HashSet<String> = recent_runs
                .iter()
                .map(|r| r.workflow_name.clone())
                .collect();

            if let Ok(mut logs_guard) = logs.try_lock() {
                for workflow_name in workflow_names {
                    if let Ok(historical_logs) =
                        history.get_logs_for_workflow(&workflow_name, Some(100))
                    {
                        logs_guard.insert(workflow_name, historical_logs);
                    }
                }
            }
        }
    }
}


async fn start_cron_scheduler(workflows_dir: &str, engine: Arc<Engine>, logs: SharedLogs) {
    let mut last_execution_times: HashMap<String, DateTime<Utc>> = HashMap::new();

    loop {
        let now = chrono::Utc::now();

        if let Ok(workflows) = WorkflowConfig::load_all(workflows_dir, Some(logs.clone())) {
            for workflow in workflows {
                if workflow.enabled && !workflow.nodes.is_empty() {
                    for trigger in &workflow.triggers {
                        if let TriggerConfig::Cron { schedule } = trigger {
                            if let Ok(cron_schedule) = cron::Schedule::from_str(schedule) {
                                // Check if this is a scheduled time
                                if let Some(next) = cron_schedule.upcoming(chrono::Utc).next() {
                                    // Check if we're at or past the scheduled time
                                    let time_to_next = (next - now).num_seconds();

                                    if time_to_next == 0 {
                                        // Create execution key based on scheduled time
                                        let execution_key = format!(
                                            "{}_{}",
                                            workflow.name,
                                            next.format("%Y%m%d%H%M")
                                        );

                                        if !last_execution_times.contains_key(&execution_key) {
                                            last_execution_times.insert(execution_key, next);

                                            // Clean up old execution records (keep only last 2 minutes)
                                            let cutoff_time = now - chrono::Duration::minutes(2);
                                            last_execution_times
                                                .retain(|_, &mut time| time > cutoff_time);

                                            // Log cron trigger with persistence
                                            log_to_persistent_storage(
                                                &logs,
                                                &workflow.name,
                                                LogLevel::Info,
                                                &format!(
                                                    "Cron triggered workflow: {}",
                                                    workflow.name
                                                ),
                                            );

                                            let engine_clone = engine.clone();
                                            let workflow_clone = workflow.clone();
                                            let workflow_name = workflow.name.clone();
                                            let logs_clone = logs.clone();
                                            tokio::spawn(async move {
                                                match engine_clone
                                                    .run_workflow(&workflow_clone)
                                                    .await
                                                {
                                                    Ok(run) => {
                                                        match run.status {
                                                            engine::RunStatus::Success => {
                                                                log_to_persistent_storage(
                                                                    &logs_clone,
                                                                    &workflow_name,
                                                                    LogLevel::Info,
                                                                    &format!("Cron workflow completed: {}", workflow_name)
                                                                );
                                                            }
                                                            engine::RunStatus::Failed => {
                                                                log_to_persistent_storage(
                                                                    &logs_clone,
                                                                    &workflow_name,
                                                                    LogLevel::Error,
                                                                    &format!(
                                                                        "Cron workflow failed: {}",
                                                                        workflow_name
                                                                    ),
                                                                );
                                                            }
                                                            _ => {}
                                                        }
                                                    }
                                                    Err(e) => {
                                                        log_to_persistent_storage(
                                                            &logs_clone,
                                                            &workflow_name,
                                                            LogLevel::Error,
                                                            &format!(
                                                                "Cron workflow error {}: {}",
                                                                workflow_name, e
                                                            ),
                                                        );
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                            } else {
                                if let Ok(mut logs_guard) = logs.try_lock() {
                                    let workflow_logs = logs_guard
                                        .entry(workflow.name.clone())
                                        .or_insert_with(Vec::new);
                                    workflow_logs.push(LogEntry {
                                        timestamp: chrono::Utc::now(),
                                        level: LogLevel::Error,
                                        message: format!(
                                            "Invalid cron schedule: {} for workflow: {}",
                                            schedule, workflow.name
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // Log error to shared logs and persistent storage
            log_to_persistent_storage(
                &logs,
                "System",
                LogLevel::Error,
                &format!("Could not load workflows from {}", workflows_dir),
            );
        }

        // Check every 1 second for precise cron timing
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[derive(Parser)]
#[command(
    name = "flowt",
    about = "Terminal workflow automation engine",
    version = "0.1.0"
)]
struct Cli {
    /// Directory containing workflow YAML files (used when launching TUI by default)
    #[arg(short, long, default_value_t = default_workflows_dir())]
    dir: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a workflow file directly
    Run {
        /// Path to the workflow YAML file
        file: String,
    },
    /// Launch the TUI dashboard
    Tui {
        /// Directory containing workflow YAML files
        #[arg(default_value_t = default_workflows_dir())]
        dir: String,
    },
    /// Start the workflow service in terminal mode (logs only)
    Serve {
        /// Directory containing workflow YAML files
        #[arg(default_value_t = default_workflows_dir())]
        dir: String,
    },
    /// List workflows in a directory
    List {
        #[arg(default_value_t = default_workflows_dir())]
        dir: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file if it exists
    dotenv::dotenv().ok();

    let cli = Cli::parse();

    // Create workflows directory if it doesn't exist (when using default TUI mode)
    let workflows_dir = match &cli.command {
        Some(Commands::Tui { dir }) => dir,
        Some(Commands::Serve { dir }) => dir,
        Some(Commands::List { dir }) => dir,
        None => &cli.dir,
        _ => &cli.dir,
    };
    if let Err(e) = std::fs::create_dir_all(workflows_dir) {
        return Err(anyhow::anyhow!(
            "Failed to create workflows directory: {}",
            e
        ));
    }

    match cli.command {
        Some(Commands::Run { file }) => {
            let workflow = WorkflowConfig::load(&file)?;
            println!("▶ Running workflow: {}", workflow.name);

            let engine = Engine::new();
            let run = engine.run_workflow(&workflow).await?;

            for result in &run.node_results {
                let icon = match &result.status {
                    engine::NodeStatus::Success => "✔",
                    engine::NodeStatus::Failed(_) => "✘",
                    _ => "-",
                };
                println!("  {} [{}] {}", icon, result.node_id, result.output);
            }

            match run.status {
                engine::RunStatus::Success => println!("\n✔ Workflow completed successfully"),
                engine::RunStatus::Failed => {
                    println!("\n✘ Workflow failed");
                    std::process::exit(1);
                }
                _ => {}
            }
        }

        Some(Commands::List { dir }) => {
            let workflows = WorkflowConfig::load_all(&dir, None)?;
            println!("Workflows in {}:\n", dir);
            for wf in &workflows {
                println!("  {} — {} nodes", wf.name, wf.nodes.len());
                if !wf.description.is_empty() {
                    println!("    {}", wf.description);
                }

                // Show execution graph structure
                println!("    Execution graph:");
                for (i, node) in wf.nodes.iter().enumerate() {
                    let connector = if i == wf.nodes.len() - 1 {
                        "└─"
                    } else {
                        "├─"
                    };

                    if node.depends_on.is_empty() {
                        println!("      {} {} (entry point)", connector, node.id);
                    } else {
                        println!(
                            "      {} {} → depends on: {}",
                            connector,
                            node.id,
                            node.depends_on.join(", ")
                        );
                    }
                }
                println!();
            }
        }

        Some(Commands::Serve { dir }) => {
            // Check if service is already running
            if lock::is_engine_running() {
                return Err(anyhow::anyhow!(
                    "Flowt service is already running. Stop it first or use 'flowt tui' to connect to the running service."
                ));
            }

            // Set service status to running
            let _ = lock::set_engine_status(true);

            println!("Starting Flowt service - monitoring workflows in: {}", dir);
            println!("Logs will be displayed below. Use Ctrl+C to stop.\n");

            let engine = Arc::new(Engine::new());

            // Load historical runs from database
            let _ = engine.load_history();

            // Create shared logs
            let logs = Arc::new(Mutex::new(HashMap::new()));

            // Load historical logs from database
            load_historical_logs(&logs);

            // Add startup log entry
            log_to_persistent_storage(
                &logs,
                "System",
                LogLevel::Info,
                &format!("Flowt service started - monitoring workflows in: {}", dir),
            );

            let workflows = WorkflowConfig::load_all(&dir, Some(logs.clone())).unwrap_or_default();

            // Start cron scheduler in background
            let engine_cron = engine.clone();
            let dir_cron = dir.clone();
            let logs_cron = logs.clone();
            tokio::spawn(async move {
                start_cron_scheduler(&dir_cron, engine_cron, logs_cron).await;
            });

            // Run all enabled manual-trigger workflows in background
            let mut auto_run_count = 0;
            for wf in workflows {
                if wf.enabled {
                    // Only auto-run workflows with manual triggers
                    let has_manual_trigger = wf
                        .triggers
                        .iter()
                        .any(|t| matches!(t, TriggerConfig::Manual));
                    if has_manual_trigger {
                        auto_run_count += 1;
                        let workflow_name = wf.name.clone();
                        let engine_clone = engine.clone();
                        let logs_clone = logs.clone();
                        tokio::spawn(async move {
                            log_to_persistent_storage(
                                &logs_clone,
                                &workflow_name,
                                LogLevel::Info,
                                &format!("Auto-starting workflow: {}", workflow_name),
                            );
                            let _ = engine_clone.run_workflow(&wf).await;
                        });
                    }
                }
            }

            if auto_run_count > 0 {
                log_to_persistent_storage(
                    &logs,
                    "System",
                    LogLevel::Info,
                    &format!("Started {} workflows automatically", auto_run_count),
                );
                println!("✔ Started {} workflows automatically", auto_run_count);
            }

            // Start log monitoring task to display logs in terminal
            let logs_monitor = logs.clone();
            let mut last_log_count = HashMap::<String, usize>::new();

            // Setup signal handling for graceful shutdown
            let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

            // Spawn signal handler
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                println!("\nReceived shutdown signal, stopping service...");
                let _ = shutdown_tx.send(()).await;
            });

            // Log monitoring loop
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        // Clean up service status
                        let _ = lock::set_engine_status(false);
                        println!("Flowt service stopped");
                        break;
                    },
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        // Periodically reload logs from database to catch TUI-triggered workflows
                        if let Ok(history) = StorageService::new() {
                            if let Ok(recent_runs) = history.get_recent_workflow_runs(Some(50)) {
                                let workflow_names: std::collections::HashSet<String> = recent_runs
                                    .iter()
                                    .map(|r| r.workflow_name.clone())
                                    .collect();

                                if let Ok(mut logs_guard) = logs_monitor.try_lock() {
                                    for workflow_name in workflow_names {
                                        if let Ok(historical_logs) =
                                            history.get_logs_for_workflow(&workflow_name, Some(100))
                                        {
                                            // Update logs from database, preserving existing count logic
                                            let current_db_count = historical_logs.len();
                                            let current_memory_count = logs_guard.get(&workflow_name).map_or(0, |logs| logs.len());

                                            // Only update if database has more entries
                                            if current_db_count > current_memory_count {
                                                logs_guard.insert(workflow_name.clone(), historical_logs);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Display new logs
                        if let Ok(logs_guard) = logs_monitor.try_lock() {
                            for (workflow_name, workflow_logs) in logs_guard.iter() {
                                let current_count = workflow_logs.len();
                                let last_count = last_log_count.get(workflow_name).unwrap_or(&0);

                                if current_count > *last_count {
                                    // Display new log entries
                                    for log_entry in workflow_logs.iter().skip(*last_count) {
                                        let level_color = match log_entry.level {
                                            LogLevel::Info => "\x1b[36m",    // Cyan
                                            LogLevel::Warning => "\x1b[33m",    // Yellow
                                            LogLevel::Error => "\x1b[31m",   // Red
                                        };
                                        println!(
                                            "{}[{}]\x1b[0m \x1b[90m{}\x1b[0m {}",
                                            level_color,
                                            workflow_name,
                                            log_entry.timestamp.format("%H:%M:%S"),
                                            log_entry.message
                                        );
                                    }
                                    last_log_count.insert(workflow_name.clone(), current_count);
                                }
                            }
                        }
                    }
                }
            }
        }

        Some(Commands::Tui { dir }) => {
            if lock::is_engine_running() {
                println!("Connecting to running Flowt service...");

                // In service mode: TUI connects to existing service - no engine startup
                let engine = Arc::new(Engine::new());
                let _ = engine.load_history();
                let runs = engine.runs.clone();

                // Create shared logs and load from database only
                let logs = Arc::new(Mutex::new(HashMap::new()));
                load_historical_logs(&logs);

                // Add connection log entry
                log_to_persistent_storage(
                    &logs,
                    "System",
                    LogLevel::Info,
                    &format!("TUI connected to running service - workflows in: {}", dir),
                );

                let mut app = tui::App::new(runs, dir.clone(), engine.clone(), logs.clone());

                // Set service mode flag to prevent duplicate scheduling
                app.service_mode = true;

                loop {
                    app.run()?;

                    if app.engine_takeover_requested {
                        // Engine died — this TUI claims the engine role
                        app.engine_takeover_requested = false;
                        app.service_mode = false;
                        let engine_cron = engine.clone();
                        let dir_cron = dir.clone();
                        let logs_cron = logs.clone();
                        tokio::spawn(async move {
                            start_cron_scheduler(&dir_cron, engine_cron, logs_cron).await;
                        });
                        // Continue loop — TUI restarts as full engine
                    } else if let Some(file_path) = app.edit_requested.clone() {
                        // Exit TUI cleanly
                        disable_raw_mode()?;
                        execute!(std::io::stdout(), LeaveAlternateScreen)?;

                        // Edit the file
                        tui::App::edit_workflow_external(&file_path);

                        // Reset edit request and restart TUI
                        app.edit_requested = None;
                        enable_raw_mode()?;
                        execute!(std::io::stdout(), EnterAlternateScreen)?;
                    } else {
                        break; // Normal exit
                    }
                }

                // Release engine lock if this TUI took over as engine
                if !app.service_mode {
                    let _ = lock::set_engine_status(false);
                }
            } else {
                // No service running: TUI acts as full engine with cron scheduling
                // Claim the engine lock so other TUI instances start as UI-only
                let _ = lock::set_engine_status(true);

                let engine = Arc::new(Engine::new());

                // Load historical runs from database
                let _ = engine.load_history();

                let runs = engine.runs.clone();

                // Create shared logs
                let logs = Arc::new(Mutex::new(HashMap::new()));

                // Load historical logs from database
                load_historical_logs(&logs);

                // Add startup log entry
                log_to_persistent_storage(
                    &logs,
                    "System",
                    LogLevel::Info,
                    &format!(
                        "TUI started with full engine - monitoring workflows in: {}",
                        dir
                    ),
                );

                let workflows =
                    WorkflowConfig::load_all(&dir, Some(logs.clone())).unwrap_or_default();

                // Start cron scheduler in background (TUI acts as full engine)
                let engine_cron = engine.clone();
                let dir_cron = dir.clone();
                let logs_cron = logs.clone();
                tokio::spawn(async move {
                    start_cron_scheduler(&dir_cron, engine_cron, logs_cron).await;
                });

                // Run all enabled manual-trigger workflows in background
                let mut auto_run_count = 0;
                for wf in workflows {
                    if wf.enabled {
                        // Only auto-run workflows with manual triggers
                        let has_manual_trigger = wf
                            .triggers
                            .iter()
                            .any(|t| matches!(t, TriggerConfig::Manual));
                        if has_manual_trigger {
                            auto_run_count += 1;
                            let workflow_name = wf.name.clone();
                            let engine_clone = engine.clone();
                            let logs_clone = logs.clone();
                            tokio::spawn(async move {
                                log_to_persistent_storage(
                                    &logs_clone,
                                    &workflow_name,
                                    LogLevel::Info,
                                    &format!("Auto-starting workflow: {}", workflow_name),
                                );
                                let _ = engine_clone.run_workflow(&wf).await;
                            });
                        }
                    }
                }

                if auto_run_count > 0 {
                    log_to_persistent_storage(
                        &logs,
                        "System",
                        LogLevel::Info,
                        &format!("Started {} workflows automatically", auto_run_count),
                    );
                }

                let mut app = tui::App::new(runs, dir.clone(), engine.clone(), logs.clone());
                loop {
                    app.run()?;

                    // Check if edit was requested
                    if let Some(file_path) = app.edit_requested.clone() {
                        // Exit TUI cleanly
                        disable_raw_mode()?;
                        execute!(std::io::stdout(), LeaveAlternateScreen)?;

                        // Edit the file
                        tui::App::edit_workflow_external(&file_path);

                        // Reset edit request and restart TUI
                        app.edit_requested = None;
                        enable_raw_mode()?;
                        execute!(std::io::stdout(), EnterAlternateScreen)?;
                    } else {
                        break; // Normal exit
                    }
                }

                // Release engine lock on exit
                let _ = lock::set_engine_status(false);
            }
        }

        None => {
            if lock::is_engine_running() {
                println!("🔗 Connecting to running Flowt service...");

                // In service mode: TUI connects to existing service - no engine startup
                let engine = Arc::new(Engine::new());
                let _ = engine.load_history();
                let runs = engine.runs.clone();

                // Create shared logs and load from database only
                let logs = Arc::new(Mutex::new(HashMap::new()));
                load_historical_logs(&logs);

                // Add connection log entry
                log_to_persistent_storage(
                    &logs,
                    "System",
                    LogLevel::Info,
                    &format!(
                        "TUI connected to running service - workflows in: {}",
                        cli.dir
                    ),
                );

                let mut app = tui::App::new(runs, cli.dir.clone(), engine.clone(), logs.clone());

                // Set service mode flag to prevent duplicate scheduling
                app.service_mode = true;

                loop {
                    app.run()?;

                    if app.engine_takeover_requested {
                        // Engine died — this TUI claims the engine role
                        app.engine_takeover_requested = false;
                        app.service_mode = false;
                        let engine_cron = engine.clone();
                        let dir_cron = cli.dir.clone();
                        let logs_cron = logs.clone();
                        tokio::spawn(async move {
                            start_cron_scheduler(&dir_cron, engine_cron, logs_cron).await;
                        });
                        // Continue loop — TUI restarts as full engine
                    } else if let Some(file_path) = app.edit_requested.clone() {
                        // Exit TUI cleanly
                        disable_raw_mode()?;
                        execute!(std::io::stdout(), LeaveAlternateScreen)?;

                        // Edit the file
                        tui::App::edit_workflow_external(&file_path);

                        // Reset edit request and restart TUI
                        app.edit_requested = None;
                        enable_raw_mode()?;
                        execute!(std::io::stdout(), EnterAlternateScreen)?;
                    } else {
                        break; // Normal exit
                    }
                }

                // Release engine lock if this TUI took over as engine
                if !app.service_mode {
                    let _ = lock::set_engine_status(false);
                }
            } else {
                // Default to TUI mode with the default directory (original behavior)
                // Claim the engine lock so other TUI instances start as UI-only
                let _ = lock::set_engine_status(true);

                let engine = Arc::new(Engine::new());

                // Load historical runs from database
                let _ = engine.load_history();

                let runs = engine.runs.clone();

                // Create shared logs
                let logs = Arc::new(Mutex::new(HashMap::new()));

                // Load historical logs from database
                load_historical_logs(&logs);

                // Add startup log entry
                // Add startup log entry
                log_to_persistent_storage(
                    &logs,
                    "System",
                    LogLevel::Info,
                    &format!(
                        "TUI started with full engine - monitoring workflows in: {}",
                        cli.dir
                    ),
                );

                // Start cron scheduler in background (TUI acts as full engine when no service running)
                let engine_cron = engine.clone();
                let dir_cron = cli.dir.clone();
                let logs_cron = logs.clone();
                tokio::spawn(async move {
                    start_cron_scheduler(&dir_cron, engine_cron, logs_cron).await;
                });

                // Manual workflows are triggered explicitly through the TUI interface
                // No auto-execution of manual workflows at startup

                let mut app = tui::App::new(runs, cli.dir.clone(), engine.clone(), logs.clone());
                loop {
                    app.run()?;

                    // Check if edit was requested
                    if let Some(file_path) = app.edit_requested.clone() {
                        // Exit TUI cleanly
                        disable_raw_mode()?;
                        execute!(std::io::stdout(), LeaveAlternateScreen)?;

                        // Edit the file
                        tui::App::edit_workflow_external(&file_path);

                        // Reset edit request and restart TUI
                        app.edit_requested = None;
                        enable_raw_mode()?;
                        execute!(std::io::stdout(), EnterAlternateScreen)?;
                    } else {
                        break; // Normal exit
                    }
                }

                // Release engine lock on exit
                let _ = lock::set_engine_status(false);
            }
        }
    }

    Ok(())
}
