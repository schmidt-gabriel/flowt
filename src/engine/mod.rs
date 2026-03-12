use crate::config::{NodeConfig, NodeKind, WorkflowConfig};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use regex::Regex;

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
    pub response_data: Option<Value>,
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
                    response_data: None,
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
        context: &HashMap<String, NodeResult>,
        run_id: &str,
    ) -> NodeResult {
        let started_at = Utc::now();

        let (status, output, response_data) = match &node.kind {
            NodeKind::Http { url, method, expect_status, headers, body } => {
                let interpolated_url = interpolate_template(url, context);
                let interpolated_headers = interpolate_headers(headers, context);
                let interpolated_body = body.as_ref().map(|b| interpolate_template(b, context));
                run_http(&interpolated_url, method, *expect_status, &interpolated_headers, interpolated_body.as_deref(), run_id).await
            }
            NodeKind::Shell { cmd, env } => {
                let interpolated_cmd = interpolate_template(cmd, context);
                let interpolated_env = interpolate_env(env, context);
                let (status, output) = run_shell(&interpolated_cmd, &interpolated_env).await;
                (status, output, None)
            }
            NodeKind::Slack { webhook_url, message } => {
                let interpolated_url = interpolate_template(webhook_url, context);
                let interpolated_message = interpolate_template(message, context);
                let (status, output) = run_slack(&interpolated_url, &interpolated_message).await;
                (status, output, None)
            }
            NodeKind::Log { message } => {
                let interpolated_message = interpolate_template(message, context);
                (NodeStatus::Success, interpolated_message, None)
            }
        };

        NodeResult {
            node_id: node.id.clone(),
            status,
            output,
            response_data,
            started_at,
            finished_at: Some(Utc::now()),
        }
    }
}

async fn run_http(
    url: &str,
    method: &str,
    expect_status: Option<u16>,
    headers: &HashMap<String, String>,
    body: Option<&str>,
    run_id: &str,
) -> (NodeStatus, String, Option<Value>) {
    let expected = expect_status.unwrap_or(200);
    let cache_dir = default_cache_dir();
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
        return (
            NodeStatus::Failed(format!("Failed to create cache directory: {}", e)),
            String::new(),
            None,
        );
    }

    let cache_file = format!("{}/flowt_{}", cache_dir, run_id);
    
    let mut cmd = format!("curl -s -o '{}' -w '%{{http_code}}' -X {}", cache_file, method.to_uppercase());
    
    // Add headers
    for (key, value) in headers {
        cmd.push_str(&format!(" -H '{}: {}'", key, value));
    }
    
    // Add body if provided
    if let Some(body_content) = body {
        cmd.push_str(&format!(" -d '{}'", body_content.replace("'", "'\"'\"'")));
    }
    
    cmd.push_str(&format!(" '{}'", url));

    match run_shell(&cmd, &HashMap::new()).await {
        (NodeStatus::Success, output) => {
            let status_code: u16 = output.trim().parse().unwrap_or(0);
            let out = format!("HTTP {}", status_code);

            if status_code == expected {
                // Try to parse JSON response
                let response_data = std::fs::read_to_string(&cache_file)
                    .ok()
                    .and_then(|content| serde_json::from_str(&content).ok());
                
                (NodeStatus::Success, out, response_data)
            } else {
                (
                    NodeStatus::Failed(format!("Expected {}, got {}", expected, status_code)),
                    out,
                    None,
                )
            }
        }
        (status, output) => (status, output, None),
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

// Template interpolation functions
fn interpolate_template(template: &str, context: &HashMap<String, NodeResult>) -> String {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    
    re.replace_all(template, |caps: &regex::Captures| {
        let var_path = &caps[1];
        
        // Handle environment variables
        if !var_path.starts_with("steps.") {
            return std::env::var(var_path).unwrap_or_else(|_| format!("${{{}}}", var_path));
        }
        
        // Handle step references: steps.step-id.response.field
        let parts: Vec<&str> = var_path.split('.').collect();
        if parts.len() >= 3 && parts[0] == "steps" {
            let step_id = parts[1];
            
            if let Some(node_result) = context.get(step_id) {
                if parts[2] == "response" && parts.len() >= 4 {
                    // Access response data field
                    if let Some(response_data) = &node_result.response_data {
                        let field_path = &parts[3..];
                        if let Some(value) = get_nested_value(response_data, field_path) {
                            return value_to_string(&value);
                        }
                    }
                } else if parts[2] == "output" {
                    return node_result.output.clone();
                } else if parts[2] == "status" {
                    return format!("{:?}", node_result.status);
                }
            }
        }
        
        format!("${{{}}}", var_path)
    }).to_string()
}

fn interpolate_headers(headers: &HashMap<String, String>, context: &HashMap<String, NodeResult>) -> HashMap<String, String> {
    headers.iter()
        .map(|(k, v)| (k.clone(), interpolate_template(v, context)))
        .collect()
}

fn interpolate_env(env: &HashMap<String, String>, context: &HashMap<String, NodeResult>) -> HashMap<String, String> {
    env.iter()
        .map(|(k, v)| (k.clone(), interpolate_template(v, context)))
        .collect()
}

fn get_nested_value(value: &Value, path: &[&str]) -> Option<Value> {
    let mut current = value;
    
    for &key in path {
        match current {
            Value::Object(obj) => {
                current = obj.get(key)?;
            }
            Value::Array(arr) => {
                if let Ok(index) = key.parse::<usize>() {
                    current = arr.get(index)?;
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }
    
    Some(current.clone())
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        _ => serde_json::to_string(value).unwrap_or_else(|_| "[complex_value]".to_string()),
    }
}
