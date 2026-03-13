use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub triggers: Vec<TriggerConfig>,
    pub nodes: Vec<NodeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerConfig {
    Cron { schedule: String },
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub id: String,
    #[serde(flatten)]
    pub kind: NodeKind,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub retry: Option<u32>,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>, // Node IDs that this node depends on
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeKind {
    Http {
        url: String,
        #[serde(default = "default_method")]
        method: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        expect_status: Option<u16>,
    },
    Shell {
        cmd: String,
        #[serde(default)]
        env: HashMap<String, String>,
    },
    Log {
        message: String,
    },
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_enabled() -> bool {
    true
}

impl WorkflowConfig {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Could not read workflow file: {:?}", path.as_ref()))?;
        let config: WorkflowConfig =
            serde_yaml::from_str(&content).context("Failed to parse workflow YAML")?;
        Ok(config)
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content =
            serde_yaml::to_string(self).context("Failed to serialize workflow to YAML")?;
        fs::write(path.as_ref(), content)
            .with_context(|| format!("Could not write workflow file: {:?}", path.as_ref()))?;
        Ok(())
    }

    pub fn toggle_enabled(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn load_all<P: AsRef<Path>>(
        dir: P,
        logs: Option<Arc<Mutex<HashMap<String, Vec<crate::tui::LogEntry>>>>>,
    ) -> Result<Vec<Self>> {
        let mut workflows = vec![];
        let entries = fs::read_dir(dir.as_ref()).context("Could not read workflows directory")?;

        for entry in entries.flatten() {
            let path = entry.path();
            // Check for both .yaml and .yml extensions
            let extension = path.extension().and_then(|e| e.to_str());
            if extension == Some("yaml") || extension == Some("yml") {
                match Self::load(&path) {
                    Ok(wf) => {
                        workflows.push(wf);
                    }
                    Err(e) => {
                        let error_msg =
                            format!("✗ Failed to load workflow {:?}: {}", path, e);

                        if let Some(logs_ref) = &logs {
                            // Log to the shared logs if available
                            if let Ok(mut logs_guard) = logs_ref.try_lock() {
                                let workflow_logs = logs_guard
                                    .entry("System".to_string())
                                    .or_insert_with(Vec::new);
                                let entry = crate::tui::LogEntry {
                                    timestamp: Utc::now(),
                                    level: crate::tui::LogLevel::Error,
                                    message: error_msg.clone(),
                                };
                                workflow_logs.push(entry);
                            }
                        } else {
                            // Fallback to stderr if logs not available
                            eprintln!("{}", &error_msg);
                        }
                        return Err(anyhow::anyhow!(error_msg));
                    }
                }
            }
        }

        // Sort workflows by name for consistent ordering
        workflows.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(workflows)
    }
}
