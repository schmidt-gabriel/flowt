use flowt::config::{NodeConfig, NodeKind, TriggerConfig, WorkflowConfig};
use std::collections::HashMap;
use std::fs::write;
use tempfile::TempDir;

#[test]
fn test_workflow_config_creation() {
    let config = WorkflowConfig {
        name: "test_workflow".to_string(),
        description: "A test workflow".to_string(),
        enabled: true,
        triggers: vec![TriggerConfig::Manual],
        nodes: vec![NodeConfig {
            id: "test_node".to_string(),
            kind: NodeKind::Log {
                message: "Hello, World!".to_string(),
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec![],
        }],
    };

    assert_eq!(config.name, "test_workflow");
    assert_eq!(config.description, "A test workflow");
    assert!(config.enabled);
    assert_eq!(config.nodes.len(), 1);
}

#[test]
fn test_workflow_config_defaults() {
    let yaml = r#"
name: test
triggers:
  - type: manual
nodes:
  - id: node1
    type: log
    message: test
"#;
    let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.name, "test");
    assert_eq!(config.description, ""); // Default empty description
    assert!(config.enabled); // Default enabled = true
    assert_eq!(config.triggers.len(), 1);
    assert_eq!(config.nodes.len(), 1);
}

#[test]
fn test_toggle_enabled() {
    let mut config = WorkflowConfig {
        name: "test".to_string(),
        description: "".to_string(),
        enabled: true,
        triggers: vec![],
        nodes: vec![],
    };

    config.toggle_enabled();
    assert!(!config.enabled);

    config.toggle_enabled();
    assert!(config.enabled);
}

#[test]
fn test_save_and_load_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_workflow.yaml");
    let file_path_str = file_path.to_str().unwrap();

    let config = WorkflowConfig {
        name: "test_save_load".to_string(),
        description: "Test save/load functionality".to_string(),
        enabled: false,
        triggers: vec![TriggerConfig::Cron {
            schedule: "0 */2 * * *".to_string(),
        }],
        nodes: vec![
            NodeConfig {
                id: "http_node".to_string(),
                kind: NodeKind::Http {
                    url: "https://api.example.com/health".to_string(),
                    method: "GET".to_string(),
                    headers: HashMap::new(),
                    body: None,
                    expect_status: Some(200),
                },
                when: Some("always".to_string()),
                retry: Some(3),
                timeout: Some("30s".to_string()),
                depends_on: vec![],
            },
            NodeConfig {
                id: "shell_node".to_string(),
                kind: NodeKind::Shell {
                    cmd: "echo 'Hello World'".to_string(),
                    env: HashMap::from([("VAR1".to_string(), "value1".to_string())]),
                },
                when: None,
                retry: None,
                timeout: None,
                depends_on: vec!["http_node".to_string()],
            },
        ],
    };

    // Test saving
    config.save(file_path_str).unwrap();
    assert!(file_path.exists());

    // Test loading
    let loaded_config = WorkflowConfig::load(file_path_str).unwrap();
    assert_eq!(loaded_config.name, "test_save_load");
    assert_eq!(loaded_config.description, "Test save/load functionality");
    assert!(!loaded_config.enabled);
    assert_eq!(loaded_config.nodes.len(), 2);

    // Check trigger
    match &loaded_config.triggers[0] {
        TriggerConfig::Cron { schedule } => {
            assert_eq!(schedule, "0 */2 * * *");
        }
        _ => panic!("Expected Cron trigger"),
    }

    // Check first node
    let first_node = &loaded_config.nodes[0];
    assert_eq!(first_node.id, "http_node");
    match &first_node.kind {
        NodeKind::Http {
            url,
            method,
            expect_status,
            ..
        } => {
            assert_eq!(url, "https://api.example.com/health");
            assert_eq!(method, "GET");
            assert_eq!(*expect_status, Some(200));
        }
        _ => panic!("Expected Http node"),
    }
    assert_eq!(first_node.retry, Some(3));

    // Check second node
    let second_node = &loaded_config.nodes[1];
    assert_eq!(second_node.id, "shell_node");
    assert_eq!(second_node.depends_on, vec!["http_node"]);
    match &second_node.kind {
        NodeKind::Shell { cmd, env } => {
            assert_eq!(cmd, "echo 'Hello World'");
            assert_eq!(env.get("VAR1"), Some(&"value1".to_string()));
        }
        _ => panic!("Expected Shell node"),
    }
}

#[test]
fn test_load_all_workflows() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Create multiple workflow files
    let workflow1_yaml = r#"
name: workflow1
triggers:
  - type: manual
nodes:
  - id: node1
    type: log
    message: "Workflow 1"
"#;

    let workflow2_yaml = r#"
name: workflow2
enabled: false
triggers:
  - type: cron
    schedule: "0 */6 * * *"
nodes:
  - id: node1
    type: shell
    cmd: "echo 'Workflow 2'"
"#;

    write(dir_path.join("workflow1.yaml"), workflow1_yaml).unwrap();
    write(dir_path.join("workflow2.yml"), workflow2_yaml).unwrap();
    write(dir_path.join("not_yaml.txt"), "not a yaml file").unwrap();

    let workflows = WorkflowConfig::load_all(dir_path.to_str().unwrap(), None).unwrap();

    // Should load 2 valid workflows, skip non-YAML files
    assert_eq!(workflows.len(), 2);

    let workflow_names: Vec<&str> = workflows.iter().map(|w| w.name.as_str()).collect();
    assert!(workflow_names.contains(&"workflow1"));
    assert!(workflow_names.contains(&"workflow2"));

    // Check specific workflows
    let wf1 = workflows.iter().find(|w| w.name == "workflow1").unwrap();
    assert!(wf1.enabled);

    let wf2 = workflows.iter().find(|w| w.name == "workflow2").unwrap();
    assert!(!wf2.enabled);
}

#[test]
fn test_node_kind_serialization() {
    // Test HTTP node
    let http_node = NodeKind::Http {
        url: "https://example.com".to_string(),
        method: "POST".to_string(),
        headers: HashMap::from([("Authorization".to_string(), "Bearer token".to_string())]),
        body: Some(r#"{"key": "value"}"#.to_string()),
        expect_status: Some(201),
    };

    let yaml = serde_yaml::to_string(&http_node).unwrap();
    let deserialized: NodeKind = serde_yaml::from_str(&yaml).unwrap();

    match deserialized {
        NodeKind::Http {
            url,
            method,
            headers,
            body,
            expect_status,
        } => {
            assert_eq!(url, "https://example.com");
            assert_eq!(method, "POST");
            assert_eq!(
                headers.get("Authorization"),
                Some(&"Bearer token".to_string())
            );
            assert_eq!(body, Some(r#"{"key": "value"}"#.to_string()));
            assert_eq!(expect_status, Some(201));
        }
        _ => panic!("Expected Http node"),
    }

    // Test Log node
    let log_node = NodeKind::Log {
        message: "Test message".to_string(),
    };
    let yaml = serde_yaml::to_string(&log_node).unwrap();
    let deserialized: NodeKind = serde_yaml::from_str(&yaml).unwrap();

    match deserialized {
        NodeKind::Log { message } => assert_eq!(message, "Test message"),
        _ => panic!("Expected Log node"),
    }
}

#[test]
fn test_load_nonexistent_file() {
    let result = WorkflowConfig::load("/path/that/does/not/exist.yaml");
    assert!(result.is_err());
}

#[test]
fn test_default_method() {
    let yaml = r#"
type: http
url: "https://example.com"
"#;
    let node: NodeKind = serde_yaml::from_str(yaml).unwrap();
    match node {
        NodeKind::Http { method, .. } => assert_eq!(method, "GET"),
        _ => panic!("Expected Http node"),
    }
}

#[test]
fn test_complex_workflow_with_dependencies() {
    let yaml = r#"
name: complex_workflow
description: "A workflow with multiple node types and dependencies"
enabled: true
triggers:
  - type: cron
    schedule: "0 9 * * *"
  - type: manual
nodes:
  - id: setup
    type: shell
    cmd: "echo 'Setting up environment'"
    env:
      ENV_VAR: "test_value"
  - id: api_call
    type: http
    url: "https://api.github.com/user"
    method: "GET"
    headers:
      Authorization: "Bearer token"
    expect_status: 200
    depends_on: ["setup"]
  - id: log_result
    type: log
    message: "API call completed: ${steps.api_call.status}"
    depends_on: ["api_call"]
  - id: cleanup
    type: shell
    cmd: "echo 'Cleaning up'"
    depends_on: ["log_result"]
"#;

    let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.name, "complex_workflow");
    assert_eq!(
        config.description,
        "A workflow with multiple node types and dependencies"
    );
    assert!(config.enabled);
    assert_eq!(config.triggers.len(), 2);
    assert_eq!(config.nodes.len(), 4);

    // Verify dependencies
    let api_call = config.nodes.iter().find(|n| n.id == "api_call").unwrap();
    assert_eq!(api_call.depends_on, vec!["setup"]);

    let log_result = config.nodes.iter().find(|n| n.id == "log_result").unwrap();
    assert_eq!(log_result.depends_on, vec!["api_call"]);

    let cleanup = config.nodes.iter().find(|n| n.id == "cleanup").unwrap();
    assert_eq!(cleanup.depends_on, vec!["log_result"]);
}
