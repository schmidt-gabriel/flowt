use flowt::config::{NodeConfig, NodeKind, TriggerConfig, WorkflowConfig};
use flowt::engine::Engine;
use flowt::storage::StorageService;
use std::collections::HashMap;
use std::fs::write;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

#[tokio::test]
async fn test_complete_workflow_lifecycle() {
    let temp_dir = TempDir::new().unwrap();

    // Set up test environment
    std::env::set_var("FLOWT_DIR", temp_dir.path().to_str().unwrap());

    // Create a workflow configuration file
    let workflow_yaml = r#"
name: integration_test_workflow
description: "Integration test workflow"
enabled: true
triggers:
  - type: manual
nodes:
  - id: setup
    type: shell
    cmd: "echo 'Setting up environment'"
    env:
      TEST_VAR: "integration_test"
  - id: process
    type: log
    message: "Processing data from ${steps.setup.output}"
    depends_on: ["setup"]
  - id: cleanup
    type: shell
    cmd: "echo 'Cleanup completed'"
    depends_on: ["process"]
"#;

    let workflow_path = temp_dir.path().join("test_workflow.yaml");
    write(&workflow_path, workflow_yaml).unwrap();

    // Load the workflow
    let workflow = WorkflowConfig::load(workflow_path.to_str().unwrap()).unwrap();
    assert_eq!(workflow.name, "integration_test_workflow");
    assert_eq!(workflow.nodes.len(), 3);

    // Create engine and execute workflow
    let engine = Engine::new();
    let run_result = engine.run_workflow(&workflow).await;

    assert!(run_result.is_ok());
    let run = run_result.unwrap();

    // Verify execution results
    assert_eq!(run.workflow_name, "integration_test_workflow");
    assert_eq!(run.node_results.len(), 3);

    // Check that all nodes completed successfully
    for result in &run.node_results {
        println!(
            "Node {}: {:?} - {}",
            result.node_id, result.status, result.output
        );
        assert_eq!(result.status, flowt::engine::NodeStatus::Success);
        assert!(result.finished_at.is_some());
    }

    // Verify execution order
    let setup_result = run
        .node_results
        .iter()
        .find(|r| r.node_id == "setup")
        .unwrap();
    let process_result = run
        .node_results
        .iter()
        .find(|r| r.node_id == "process")
        .unwrap();
    let cleanup_result = run
        .node_results
        .iter()
        .find(|r| r.node_id == "cleanup")
        .unwrap();

    assert!(setup_result.finished_at.unwrap() <= process_result.started_at);
    assert!(process_result.finished_at.unwrap() <= cleanup_result.started_at);

    // Verify storage persistence
    let storage = StorageService::new().unwrap();
    let recent_runs = storage.get_recent_workflow_runs(Some(1)).unwrap();
    assert_eq!(recent_runs.len(), 1);
    assert_eq!(recent_runs[0].workflow_name, "integration_test_workflow");
}

#[tokio::test]
async fn test_workflow_with_http_node() {
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("FLOWT_DIR", temp_dir.path().to_str().unwrap());

    let workflow = WorkflowConfig {
        name: "http_test_workflow".to_string(),
        description: "Test HTTP workflow".to_string(),
        enabled: true,
        triggers: vec![TriggerConfig::Manual],
        nodes: vec![
            NodeConfig {
                id: "http_call".to_string(),
                kind: NodeKind::Http {
                    // Using httpbin.org for reliable testing
                    url: "https://httpbin.org/status/200".to_string(),
                    method: "GET".to_string(),
                    headers: HashMap::new(),
                    body: None,
                    expect_status: Some(200),
                },
                when: None,
                retry: None,
                timeout: None,
                depends_on: vec![],
            },
            NodeConfig {
                id: "log_result".to_string(),
                kind: NodeKind::Log {
                    message: "HTTP call completed".to_string(),
                },
                when: None,
                retry: None,
                timeout: None,
                depends_on: vec!["http_call".to_string()],
            },
        ],
    };

    let engine = Engine::new();
    let run_result = engine.run_workflow(&workflow).await;

    assert!(run_result.is_ok());
    let run = run_result.unwrap();

    // Check that HTTP call succeeded
    let http_result = run
        .node_results
        .iter()
        .find(|r| r.node_id == "http_call")
        .unwrap();
    assert_eq!(http_result.status, flowt::engine::NodeStatus::Success);
    assert!(http_result.output.contains("200"));
}

#[test]
fn test_load_multiple_workflows_from_directory() {
    let temp_dir = TempDir::new().unwrap();
    let workflows_dir = temp_dir.path().join("workflows");
    std::fs::create_dir_all(&workflows_dir).unwrap();

    // Create multiple workflow files
    let workflow1 = r#"
name: workflow_alpha
triggers:
  - type: manual
nodes:
  - id: task1
    type: log
    message: "Alpha workflow"
"#;

    let workflow2 = r#"
name: workflow_beta
enabled: false
triggers:
  - type: cron
    schedule: "0 */6 * * *"
nodes:
  - id: task1
    type: shell
    cmd: "echo 'Beta workflow'"
"#;

    let workflow3 = r#"
name: workflow_gamma
triggers:
  - type: manual
nodes:
  - id: task1
    type: log
    message: "Gamma workflow"
  - id: task2
    type: shell
    cmd: "echo 'Second task'"
    depends_on: ["task1"]
"#;

    write(workflows_dir.join("alpha.yaml"), workflow1).unwrap();
    write(workflows_dir.join("beta.yml"), workflow2).unwrap();
    write(workflows_dir.join("gamma.yaml"), workflow3).unwrap();
    write(
        workflows_dir.join("not_a_workflow.txt"),
        "This is not a workflow",
    )
    .unwrap();

    // Load all workflows
    let shared_logs = Arc::new(Mutex::new(HashMap::new()));
    let workflows =
        WorkflowConfig::load_all(workflows_dir.to_str().unwrap(), Some(shared_logs.clone()))
            .unwrap();

    // Should load 3 workflows (ignoring .txt file)
    assert_eq!(workflows.len(), 3);

    // Verify workflows are sorted by name
    let names: Vec<&str> = workflows.iter().map(|w| w.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["workflow_alpha", "workflow_beta", "workflow_gamma"]
    );

    // Check specific workflow properties
    let alpha = workflows
        .iter()
        .find(|w| w.name == "workflow_alpha")
        .unwrap();
    assert!(alpha.enabled);

    let beta = workflows
        .iter()
        .find(|w| w.name == "workflow_beta")
        .unwrap();
    assert!(!beta.enabled);

    let gamma = workflows
        .iter()
        .find(|w| w.name == "workflow_gamma")
        .unwrap();
    assert_eq!(gamma.nodes.len(), 2);
    assert_eq!(gamma.nodes[1].depends_on, vec!["task1"]);
}

#[tokio::test]
async fn test_engine_with_storage_persistence() {
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("FLOWT_DIR", temp_dir.path().to_str().unwrap());

    let engine = Engine::new();

    let workflow = WorkflowConfig {
        name: "persistence_test".to_string(),
        description: "Test persistence".to_string(),
        enabled: true,
        triggers: vec![TriggerConfig::Manual],
        nodes: vec![NodeConfig {
            id: "test_node".to_string(),
            kind: NodeKind::Log {
                message: "Testing persistence".to_string(),
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec![],
        }],
    };

    // Run workflow
    let run_result = engine.run_workflow(&workflow).await;
    assert!(run_result.is_ok());

    // Create new engine instance to test persistence
    let engine2 = Engine::new();
    let load_result = engine2.load_history();
    assert!(load_result.is_ok());

    // Check that the run was persisted and loaded
    let runs = engine2.runs.lock().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].workflow_name, "persistence_test");
}

#[test]
fn test_workflow_serialization_roundtrip() {
    let original = WorkflowConfig {
        name: "roundtrip_test".to_string(),
        description: "Test serialization roundtrip".to_string(),
        enabled: true,
        triggers: vec![
            TriggerConfig::Manual,
            TriggerConfig::Cron {
                schedule: "0 9 * * *".to_string(),
            },
        ],
        nodes: vec![
            NodeConfig {
                id: "http_node".to_string(),
                kind: NodeKind::Http {
                    url: "https://api.example.com".to_string(),
                    method: "POST".to_string(),
                    headers: HashMap::from([
                        ("Content-Type".to_string(), "application/json".to_string()),
                        ("Authorization".to_string(), "Bearer ${TOKEN}".to_string()),
                    ]),
                    body: Some(r#"{"data": "test"}"#.to_string()),
                    expect_status: Some(201),
                },
                when: Some("always".to_string()),
                retry: Some(3),
                timeout: Some("30s".to_string()),
                depends_on: vec![],
            },
            NodeConfig {
                id: "shell_node".to_string(),
                kind: NodeKind::Shell {
                    cmd: "echo 'Processing: ${steps.http_node.response.id}'".to_string(),
                    env: HashMap::from([
                        ("DEBUG".to_string(), "true".to_string()),
                        ("OUTPUT_DIR".to_string(), "/tmp/output".to_string()),
                    ]),
                },
                when: None,
                retry: Some(1),
                timeout: None,
                depends_on: vec!["http_node".to_string()],
            },
            NodeConfig {
                id: "log_node".to_string(),
                kind: NodeKind::Log {
                    message: "Workflow completed successfully. Status: ${steps.shell_node.status}"
                        .to_string(),
                },
                when: None,
                retry: None,
                timeout: None,
                depends_on: vec!["shell_node".to_string()],
            },
        ],
    };

    // Serialize to YAML
    let yaml_str = serde_yaml::to_string(&original).unwrap();

    // Deserialize back
    let deserialized: WorkflowConfig = serde_yaml::from_str(&yaml_str).unwrap();

    // Verify all fields match
    assert_eq!(deserialized.name, original.name);
    assert_eq!(deserialized.description, original.description);
    assert_eq!(deserialized.enabled, original.enabled);
    assert_eq!(deserialized.triggers.len(), original.triggers.len());
    assert_eq!(deserialized.nodes.len(), original.nodes.len());

    // Check node details
    for (orig_node, deser_node) in original.nodes.iter().zip(deserialized.nodes.iter()) {
        assert_eq!(orig_node.id, deser_node.id);
        assert_eq!(orig_node.when, deser_node.when);
        assert_eq!(orig_node.retry, deser_node.retry);
        assert_eq!(orig_node.timeout, deser_node.timeout);
        assert_eq!(orig_node.depends_on, deser_node.depends_on);
    }
}
