mod config;
mod engine;
mod storage;
mod tui;

use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use config::{TriggerConfig, WorkflowConfig};
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

        Some(Commands::Tui { dir }) => {
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
                &format!("Flowt started - monitoring workflows in: {}", dir),
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
            }

            let mut app = tui::App::new(runs, dir.clone(), engine.clone(), logs.clone());
            app.run()?;
        }

        None => {
            // Default to TUI mode with the default directory
            let engine = Arc::new(Engine::new());

            // Load historical runs from database
            let _ = engine.load_history();

            let runs = engine.runs.clone();

            // Create shared logs
            let logs = Arc::new(Mutex::new(HashMap::new()));

            // Load historical logs from database
            load_historical_logs(&logs);
            let logs = Arc::new(Mutex::new(HashMap::new()));

            // Add startup log entry
            if let Ok(mut logs_guard) = logs.try_lock() {
                let system_logs = logs_guard
                    .entry("System".to_string())
                    .or_insert_with(Vec::new);
                system_logs.push(LogEntry {
                    timestamp: chrono::Utc::now(),
                    level: LogLevel::Info,
                    message: format!("Flowt started - monitoring workflows in: {}", cli.dir),
                });
            }

            // Start cron scheduler in background
            let engine_cron = engine.clone();
            let dir_cron = cli.dir.clone();
            let logs_cron = logs.clone();
            tokio::spawn(async move {
                start_cron_scheduler(&dir_cron, engine_cron, logs_cron).await;
            });

            // Manual workflows are triggered explicitly through the TUI interface
            // No auto-execution of manual workflows at startup

            let mut app = tui::App::new(runs, cli.dir.clone(), engine.clone(), logs.clone());
            app.run()?;
        }
    }

    Ok(())
}
