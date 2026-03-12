use crate::config::{NodeConfig, NodeKind, WorkflowConfig};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn default_cache_dir() -> String {
    std::env::var("FLOWT_DIR")
        .map(|dir| format!("{}/.cache", dir))
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{}/.flowt/.cache", home)
        })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeStatus {
    Pending,
    Running,
    Success,
    Failed(String),
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResult {
    pub node_id: String,
    pub status: NodeStatus,
    pub output: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RunStatus {
    Running,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: String,
    pub workflow_name: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub node_results: Vec<NodeResult>,
}

impl WorkflowRun {
    pub fn new(workflow_name: &str) -> Self {
        let ts = Utc::now().timestamp_millis();
        Self {
            id: format!("{:x}", ts & 0xFFFFFF),
            workflow_name: workflow_name.to_string(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            node_results: vec![],
        }
    }
}

pub type SharedRuns = Arc<Mutex<Vec<WorkflowRun>>>;

pub struct Engine {
    pub runs: SharedRuns,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            runs: Arc::new(Mutex::new(vec![])),
        }
    }

    pub async fn run_workflow(&self, workflow: &WorkflowConfig) -> Result<WorkflowRun> {
        let mut run = WorkflowRun::new(&workflow.name);

        {
            let mut runs = self.runs.lock().unwrap();
            runs.push(run.clone());
        }

        let mut context: HashMap<String, NodeResult> = HashMap::new();
        let execution_order = self.topological_sort(&workflow.nodes)?;

        for node_id in execution_order {
            let node = workflow.nodes.iter().find(|n| n.id == node_id).unwrap();
            
            // Check if all dependencies are satisfied and successful
            let dependencies_satisfied = node.depends_on.iter().all(|dep_id| {
                context.get(dep_id).map_or(false, |result| {
                    matches!(result.status, NodeStatus::Success)
                })
            });

            let result = if dependencies_satisfied || node.depends_on.is_empty() {
                self.execute_node(node, &context, &run.id).await
            } else {
                // Skip if dependencies failed
                NodeResult {
                    node_id: node.id.clone(),
                    status: NodeStatus::Skipped,
                    output: "Skipped due to failed dependencies".to_string(),
                    started_at: Utc::now(),
                    finished_at: Some(Utc::now()),
                }
            };

            context.insert(node.id.clone(), result.clone());

            {
                let mut runs = self.runs.lock().unwrap();
                if let Some(r) = runs.iter_mut().find(|r| r.id == run.id) {
                    r.node_results.push(result.clone());
                }
            }

            run.node_results.push(result.clone());

            if matches!(result.status, NodeStatus::Failed(_)) {
                run.status = RunStatus::Failed;
                // Continue execution to show which nodes would be skipped
            }
        }

        if run.status == RunStatus::Running {
            run.status = RunStatus::Success;
        }

        run.finished_at = Some(Utc::now());

        {
            let mut runs = self.runs.lock().unwrap();
            if let Some(r) = runs.iter_mut().find(|r| r.id == run.id) {
                r.status = run.status.clone();
                r.finished_at = run.finished_at;
            }
        }

        Ok(run)
    }

    // Topological sort to determine execution order based on dependencies
    fn topological_sort(&self, nodes: &[NodeConfig]) -> Result<Vec<String>> {
        let mut visited = std::collections::HashSet::new();
        let mut temp_visited = std::collections::HashSet::new();
        let mut result = Vec::new();
        
        // Create a map for quick node lookup
        let node_map: HashMap<String, &NodeConfig> = nodes.iter()
            .map(|n| (n.id.clone(), n))
            .collect();

        // Recursive DFS for topological sort
        fn visit(
            node_id: &str,
            node_map: &HashMap<String, &NodeConfig>,
            visited: &mut std::collections::HashSet<String>,
            temp_visited: &mut std::collections::HashSet<String>,
            result: &mut Vec<String>,
        ) -> Result<()> {
            if temp_visited.contains(node_id) {
                return Err(anyhow::anyhow!("Circular dependency detected involving node: {}", node_id));
            }
            if visited.contains(node_id) {
                return Ok(());
            }

            temp_visited.insert(node_id.to_string());

            if let Some(node) = node_map.get(node_id) {
                for dep in &node.depends_on {
                    visit(dep, node_map, visited, temp_visited, result)?;
                }
            }

            temp_visited.remove(node_id);
            visited.insert(node_id.to_string());
            result.push(node_id.to_string());

            Ok(())
        }

        // Visit all nodes
        for node in nodes {
            if !visited.contains(&node.id) {
                visit(&node.id, &node_map, &mut visited, &mut temp_visited, &mut result)?;
            }
        }

        Ok(result)
    }

    async fn execute_node(
        &self,
        node: &NodeConfig,
        _context: &HashMap<String, NodeResult>,
        run_id: &str,
    ) -> NodeResult {
        let started_at = Utc::now();

        let (status, output) = match &node.kind {
            NodeKind::Http { url, method, expect_status, .. } => {
                run_http(url, method, *expect_status, run_id).await
            }
            NodeKind::Shell { cmd, env } => run_shell(cmd, env).await,
            NodeKind::Slack { webhook_url, message } => {
                run_slack(webhook_url, message).await
            }
            NodeKind::Log { message } => (NodeStatus::Success, message.clone()),
        };

        NodeResult {
            node_id: node.id.clone(),
            status,
            output,
            started_at,
            finished_at: Some(Utc::now()),
        }
    }
}

async fn run_http(url: &str, method: &str, expect_status: Option<u16>, run_id: &str) -> (NodeStatus, String) {
    let expected = expect_status.unwrap_or(200);
    let cache_dir = default_cache_dir();
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
        return (NodeStatus::Failed(format!("Failed to create cache directory: {}", e)), String::new());
    }
    
    let cmd = format!(
        "curl -s -o {}/flowt_{} -w '%{{http_code}}' -X {} '{}'",
        cache_dir,
        run_id,
        method.to_uppercase(),
        url
    );

    match run_shell(&cmd, &HashMap::new()).await {
        (NodeStatus::Success, output) => {
            let status_code: u16 = output.trim().parse().unwrap_or(0);
            let out = format!("HTTP {}", status_code);

            if status_code == expected {
                (NodeStatus::Success, out)
            } else {
                (
                    NodeStatus::Failed(format!("Expected {}, got {}", expected, status_code)),
                    out,
                )
            }
        }
        (status, output) => (status, output),
    }
}

async fn run_shell(cmd: &str, env: &HashMap<String, String>) -> (NodeStatus, String) {
    let mut command = tokio::process::Command::new("sh");
    command.arg("-c").arg(cmd);
    for (k, v) in env {
        command.env(k, v);
    }

    match command.output().await {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let output = format!("{}{}", stdout, stderr).trim().to_string();

            if out.status.success() {
                (NodeStatus::Success, output)
            } else {
                (
                    NodeStatus::Failed(format!("Exit {}", out.status)),
                    output,
                )
            }
        }
        Err(e) => (NodeStatus::Failed(e.to_string()), String::new()),
    }
}

async fn run_slack(webhook_url: &str, message: &str) -> (NodeStatus, String) {
    let body = format!("{{\"text\":\"{}\"}}", message.replace('"', "\\\""));
    let cmd = format!(
        "curl -s -X POST -H 'Content-Type: application/json' -d '{}' '{}'",
        body, webhook_url
    );
    run_shell(&cmd, &HashMap::new()).await
}
