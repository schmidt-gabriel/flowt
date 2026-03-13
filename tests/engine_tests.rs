use flowt::config::{NodeConfig, NodeKind, TriggerConfig, WorkflowConfig};
use flowt::engine::{Engine, NodeResult, NodeStatus, RunStatus, WorkflowRun};
use std::collections::HashMap;

#[test]
fn test_workflow_run_creation() {
    let run = WorkflowRun::new("test_workflow");

    assert_eq!(run.workflow_name, "test_workflow");
    assert_eq!(run.status, RunStatus::Running);
    assert!(run.node_results.is_empty());
    assert!(run.finished_at.is_none());
    assert!(!run.id.is_empty());
}

#[test]
fn test_engine_creation() {
    let engine = Engine::new();
    let runs = engine.runs.lock().unwrap();
    assert!(runs.is_empty());
}

#[test]
fn test_node_status_equality() {
    assert_eq!(NodeStatus::Pending, NodeStatus::Pending);
    assert_eq!(NodeStatus::Running, NodeStatus::Running);
    assert_eq!(NodeStatus::Success, NodeStatus::Success);
    assert_eq!(NodeStatus::Skipped, NodeStatus::Skipped);
    assert_eq!(
        NodeStatus::Failed("error".to_string()),
        NodeStatus::Failed("error".to_string())
    );
    assert_ne!(
        NodeStatus::Failed("error1".to_string()),
        NodeStatus::Failed("error2".to_string())
    );
}

#[test]
fn test_run_status_equality() {
    assert_eq!(RunStatus::Running, RunStatus::Running);
    assert_eq!(RunStatus::Success, RunStatus::Success);
    assert_eq!(RunStatus::Failed, RunStatus::Failed);
    assert_ne!(RunStatus::Running, RunStatus::Success);
}

#[tokio::test]
async fn test_engine_topological_sort_simple() {
    let engine = Engine::new();

    let nodes = vec![
        NodeConfig {
            id: "node1".to_string(),
            kind: NodeKind::Log {
                message: "First".to_string(),
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec![],
        },
        NodeConfig {
            id: "node2".to_string(),
            kind: NodeKind::Log {
                message: "Second".to_string(),
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec!["node1".to_string()],
        },
    ];

    let result = engine.topological_sort(&nodes);
    assert!(result.is_ok());

    let order = result.unwrap();
    assert_eq!(order.len(), 2);
    assert_eq!(order[0], "node1");
    assert_eq!(order[1], "node2");
}

#[tokio::test]
async fn test_engine_topological_sort_complex() {
    let engine = Engine::new();

    let nodes = vec![
        NodeConfig {
            id: "setup".to_string(),
            kind: NodeKind::Log {
                message: "Setup".to_string(),
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec![],
        },
        NodeConfig {
            id: "task1".to_string(),
            kind: NodeKind::Log {
                message: "Task 1".to_string(),
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec!["setup".to_string()],
        },
        NodeConfig {
            id: "task2".to_string(),
            kind: NodeKind::Log {
                message: "Task 2".to_string(),
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec!["setup".to_string()],
        },
        NodeConfig {
            id: "cleanup".to_string(),
            kind: NodeKind::Log {
                message: "Cleanup".to_string(),
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec!["task1".to_string(), "task2".to_string()],
        },
    ];

    let result = engine.topological_sort(&nodes);
    assert!(result.is_ok());

    let order = result.unwrap();
    assert_eq!(order.len(), 4);

    // Setup should be first
    assert_eq!(order[0], "setup");

    // cleanup should be last
    assert_eq!(order[3], "cleanup");

    // task1 and task2 should come after setup and before cleanup
    let setup_index = order.iter().position(|x| x == "setup").unwrap();
    let task1_index = order.iter().position(|x| x == "task1").unwrap();
    let task2_index = order.iter().position(|x| x == "task2").unwrap();
    let cleanup_index = order.iter().position(|x| x == "cleanup").unwrap();

    assert!(task1_index > setup_index);
    assert!(task2_index > setup_index);
    assert!(cleanup_index > task1_index);
    assert!(cleanup_index > task2_index);
}

#[tokio::test]
async fn test_engine_topological_sort_circular_dependency() {
    let engine = Engine::new();

    let nodes = vec![
        NodeConfig {
            id: "node1".to_string(),
            kind: NodeKind::Log {
                message: "First".to_string(),
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec!["node2".to_string()],
        },
        NodeConfig {
            id: "node2".to_string(),
            kind: NodeKind::Log {
                message: "Second".to_string(),
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec!["node1".to_string()],
        },
    ];

    let result = engine.topological_sort(&nodes);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Circular dependency"));
}

#[tokio::test]
async fn test_workflow_execution_log_node() {
    let engine = Engine::new();

    let workflow = WorkflowConfig {
        name: "test_log_workflow".to_string(),
        description: "Test workflow with log node".to_string(),
        enabled: true,
        triggers: vec![TriggerConfig::Manual],
        nodes: vec![NodeConfig {
            id: "log_hello".to_string(),
            kind: NodeKind::Log {
                message: "Hello from test!".to_string(),
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec![],
        }],
    };

    let result = engine.run_workflow(&workflow).await;
    assert!(result.is_ok());

    let run = result.unwrap();
    assert_eq!(run.workflow_name, "test_log_workflow");
    assert_eq!(run.status, RunStatus::Success);
    assert!(run.finished_at.is_some());
    assert_eq!(run.node_results.len(), 1);

    let node_result = &run.node_results[0];
    assert_eq!(node_result.node_id, "log_hello");
    assert_eq!(node_result.status, NodeStatus::Success);
    assert_eq!(node_result.output, "Hello from test!");
    assert!(node_result.finished_at.is_some());
}

#[tokio::test]
async fn test_workflow_execution_shell_node() {
    let engine = Engine::new();

    let workflow = WorkflowConfig {
        name: "test_shell_workflow".to_string(),
        description: "Test workflow with shell node".to_string(),
        enabled: true,
        triggers: vec![TriggerConfig::Manual],
        nodes: vec![NodeConfig {
            id: "echo_test".to_string(),
            kind: NodeKind::Shell {
                cmd: "echo 'test output'".to_string(),
                env: HashMap::new(),
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec![],
        }],
    };

    let result = engine.run_workflow(&workflow).await;
    assert!(result.is_ok());

    let run = result.unwrap();
    assert_eq!(run.workflow_name, "test_shell_workflow");
    assert_eq!(run.status, RunStatus::Success);
    assert!(run.finished_at.is_some());
    assert_eq!(run.node_results.len(), 1);

    let node_result = &run.node_results[0];
    assert_eq!(node_result.node_id, "echo_test");
    assert_eq!(node_result.status, NodeStatus::Success);
    assert_eq!(node_result.output.trim(), "test output");
}

#[tokio::test]
async fn test_workflow_execution_with_dependencies() {
    let engine = Engine::new();

    let workflow = WorkflowConfig {
        name: "test_dependency_workflow".to_string(),
        description: "Test workflow with dependencies".to_string(),
        enabled: true,
        triggers: vec![TriggerConfig::Manual],
        nodes: vec![
            NodeConfig {
                id: "first".to_string(),
                kind: NodeKind::Log {
                    message: "First task".to_string(),
                },
                when: None,
                retry: None,
                timeout: None,
                depends_on: vec![],
            },
            NodeConfig {
                id: "second".to_string(),
                kind: NodeKind::Log {
                    message: "Second task depends on first".to_string(),
                },
                when: None,
                retry: None,
                timeout: None,
                depends_on: vec!["first".to_string()],
            },
        ],
    };

    let result = engine.run_workflow(&workflow).await;
    assert!(result.is_ok());

    let run = result.unwrap();
    assert_eq!(run.status, RunStatus::Success);
    assert_eq!(run.node_results.len(), 2);

    // Check execution order - first should complete before second starts
    let first_result = run
        .node_results
        .iter()
        .find(|r| r.node_id == "first")
        .unwrap();
    let second_result = run
        .node_results
        .iter()
        .find(|r| r.node_id == "second")
        .unwrap();

    assert_eq!(first_result.status, NodeStatus::Success);
    assert_eq!(second_result.status, NodeStatus::Success);
    assert!(first_result.finished_at.unwrap() <= second_result.started_at);
}

#[tokio::test]
async fn test_workflow_execution_failed_dependency_skips() {
    let engine = Engine::new();

    let workflow = WorkflowConfig {
        name: "test_failed_dependency".to_string(),
        description: "Test workflow where dependency fails".to_string(),
        enabled: true,
        triggers: vec![TriggerConfig::Manual],
        nodes: vec![
            NodeConfig {
                id: "failing_task".to_string(),
                kind: NodeKind::Shell {
                    cmd: "exit 1".to_string(), // This command will fail
                    env: HashMap::new(),
                },
                when: None,
                retry: None,
                timeout: None,
                depends_on: vec![],
            },
            NodeConfig {
                id: "dependent_task".to_string(),
                kind: NodeKind::Log {
                    message: "This should be skipped".to_string(),
                },
                when: None,
                retry: None,
                timeout: None,
                depends_on: vec!["failing_task".to_string()],
            },
        ],
    };

    let result = engine.run_workflow(&workflow).await;
    assert!(result.is_ok());

    let run = result.unwrap();
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.node_results.len(), 2);

    let failing_result = run
        .node_results
        .iter()
        .find(|r| r.node_id == "failing_task")
        .unwrap();
    let dependent_result = run
        .node_results
        .iter()
        .find(|r| r.node_id == "dependent_task")
        .unwrap();

    assert!(matches!(failing_result.status, NodeStatus::Failed(_)));
    assert_eq!(dependent_result.status, NodeStatus::Skipped);
    assert_eq!(
        dependent_result.output,
        "Skipped due to failed dependencies"
    );
}

#[tokio::test]
async fn test_workflow_execution_shell_with_env() {
    let engine = Engine::new();

    let mut env = HashMap::new();
    env.insert("TEST_VAR".to_string(), "test_value".to_string());

    let workflow = WorkflowConfig {
        name: "test_env_workflow".to_string(),
        description: "Test workflow with environment variables".to_string(),
        enabled: true,
        triggers: vec![TriggerConfig::Manual],
        nodes: vec![NodeConfig {
            id: "env_test".to_string(),
            kind: NodeKind::Shell {
                cmd: "echo $TEST_VAR".to_string(),
                env,
            },
            when: None,
            retry: None,
            timeout: None,
            depends_on: vec![],
        }],
    };

    let result = engine.run_workflow(&workflow).await;
    assert!(result.is_ok());

    let run = result.unwrap();
    let node_result = &run.node_results[0];
    assert_eq!(node_result.status, NodeStatus::Success);
    assert_eq!(node_result.output.trim(), "test_value");
}

#[test]
fn test_node_result_creation() {
    let now = chrono::Utc::now();
    let result = NodeResult {
        node_id: "test_node".to_string(),
        status: NodeStatus::Success,
        output: "test output".to_string(),
        response_data: None,
        started_at: now,
        finished_at: Some(now),
    };

    assert_eq!(result.node_id, "test_node");
    assert_eq!(result.status, NodeStatus::Success);
    assert_eq!(result.output, "test output");
    assert!(result.response_data.is_none());
    assert_eq!(result.started_at, now);
    assert_eq!(result.finished_at, Some(now));
}

#[test]
fn test_workflow_run_id_uniqueness() {
    let run1 = WorkflowRun::new("test");

    // Sleep briefly to ensure different timestamps
    std::thread::sleep(std::time::Duration::from_millis(1));

    let run2 = WorkflowRun::new("test");

    // IDs should be unique (based on timestamp)
    assert_ne!(run1.id, run2.id);
}

#[tokio::test]
async fn test_workflow_execution_when_condition() {
    let engine = Engine::new();

    let workflow = WorkflowConfig {
        name: "test_when_condition_workflow".to_string(),
        description: "Test workflow with 'when' conditions".to_string(),
        enabled: true,
        triggers: vec![TriggerConfig::Manual],
        nodes: vec![
            NodeConfig {
                id: "first".to_string(),
                kind: NodeKind::Log {
                    message: "Always runs".to_string(),
                },
                when: None,
                retry: None,
                timeout: None,
                depends_on: vec![],
            },
            NodeConfig {
                id: "second".to_string(),
                kind: NodeKind::Log {
                    message: "Runs because first succeeded".to_string(),
                },
                when: Some("first == success".to_string()),
                retry: None,
                timeout: None,
                depends_on: vec!["first".to_string()],
            },
            NodeConfig {
                id: "third".to_string(),
                kind: NodeKind::Log {
                    message: "Is skipped because first did not fail".to_string(),
                },
                when: Some("first == failed".to_string()),
                retry: None,
                timeout: None,
                depends_on: vec!["first".to_string()],
            },
        ],
    };

    let result = engine.run_workflow(&workflow).await;
    assert!(result.is_ok());

    let run = result.unwrap();
    assert_eq!(run.status, RunStatus::Success);
    assert_eq!(run.node_results.len(), 3);

    let first_result = run
        .node_results
        .iter()
        .find(|r| r.node_id == "first")
        .unwrap();
    let second_result = run
        .node_results
        .iter()
        .find(|r| r.node_id == "second")
        .unwrap();
    let third_result = run
        .node_results
        .iter()
        .find(|r| r.node_id == "third")
        .unwrap();

    assert_eq!(first_result.status, NodeStatus::Success);
    assert_eq!(second_result.status, NodeStatus::Success);
    assert_eq!(third_result.status, NodeStatus::Skipped);
}
