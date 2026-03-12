use flowt::engine::{NodeResult, NodeStatus, RunStatus, WorkflowRun};
use flowt::storage::StorageService;
use flowt::tui::{LogEntry, LogLevel};
use std::collections::HashMap;
use tempfile::TempDir;

// Helper function to create a test StorageService with a temporary database
fn create_test_storage() -> (StorageService, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    // Generate unique test ID to avoid database conflicts
    let test_id = std::thread::current().id();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    // Set environment variable to use test database with unique path
    std::env::set_var(
        "FLOWT_DIR",
        format!(
            "{}/test_{:?}_{}",
            temp_dir.path().to_str().unwrap(),
            test_id,
            timestamp
        ),
    );

    let storage = StorageService::new().unwrap();
    (storage, temp_dir)
}

#[test]
fn test_storage_service_creation() {
    let (_storage, _temp_dir) = create_test_storage();
    // If we get here without panicking, the storage service was created successfully
}

#[test]
fn test_save_and_get_workflow_run() {
    let (storage, _temp_dir) = create_test_storage();

    let mut run = WorkflowRun::new("test_workflow");
    run.status = RunStatus::Success;
    run.finished_at = Some(chrono::Utc::now());

    // Add a node result
    run.node_results.push(NodeResult {
        node_id: "test_node".to_string(),
        status: NodeStatus::Success,
        output: "Success message".to_string(),
        response_data: None,
        started_at: chrono::Utc::now(),
        finished_at: Some(chrono::Utc::now()),
    });

    // Save the workflow run
    let result = storage.save_workflow_run(&run);
    assert!(result.is_ok());

    // Get recent runs
    let recent_runs = storage.get_recent_workflow_runs(Some(10));
    assert!(recent_runs.is_ok());

    let runs = recent_runs.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].workflow_name, "test_workflow");
    assert_eq!(runs[0].status, RunStatus::Success);
    assert_eq!(runs[0].node_results.len(), 1);
    assert_eq!(runs[0].node_results[0].node_id, "test_node");
}

#[test]
fn test_update_workflow_run() {
    let (storage, _temp_dir) = create_test_storage();

    let mut run = WorkflowRun::new("test_workflow");

    // Save initial run
    storage.save_workflow_run(&run).unwrap();

    // Update the run
    run.status = RunStatus::Success;
    run.finished_at = Some(chrono::Utc::now());
    run.node_results.push(NodeResult {
        node_id: "updated_node".to_string(),
        status: NodeStatus::Success,
        output: "Updated output".to_string(),
        response_data: None,
        started_at: chrono::Utc::now(),
        finished_at: Some(chrono::Utc::now()),
    });

    let result = storage.update_workflow_run(&run);
    assert!(result.is_ok());

    // Verify the update
    let recent_runs = storage.get_recent_workflow_runs(Some(10)).unwrap();
    assert_eq!(recent_runs.len(), 1);
    let updated_run = &recent_runs[0];
    assert_eq!(updated_run.status, RunStatus::Success);
    assert_eq!(updated_run.node_results.len(), 1);
    assert_eq!(updated_run.node_results[0].node_id, "updated_node");
}

#[test]
fn test_save_and_get_log_entries() {
    let (storage, _temp_dir) = create_test_storage();

    let log_entry = LogEntry {
        timestamp: chrono::Utc::now(),
        level: LogLevel::Info,
        message: "Test log message".to_string(),
    };

    // Save the log entry
    let result = storage.save_log_entry("test_workflow", &log_entry);
    assert!(result.is_ok());

    // Retrieve log entries
    let logs = storage.get_logs_for_workflow("test_workflow", Some(10));
    assert!(logs.is_ok());

    let log_entries = logs.unwrap();
    assert_eq!(log_entries.len(), 1);
    assert_eq!(log_entries[0].message, "Test log message");
    assert_eq!(log_entries[0].level, LogLevel::Info);
}

#[test]
fn test_save_multiple_log_entries() {
    let (storage, _temp_dir) = create_test_storage();

    let workflows = ["workflow1", "workflow2"];
    let levels = [LogLevel::Info, LogLevel::Error, LogLevel::Warning];

    // Save multiple log entries for different workflows
    for (_i, &workflow) in workflows.iter().enumerate() {
        for (j, level) in levels.iter().enumerate() {
            let log_entry = LogEntry {
                timestamp: chrono::Utc::now(),
                level: *level,
                message: format!("Log {} for {}", j, workflow),
            };
            storage.save_log_entry(workflow, &log_entry).unwrap();
        }
    }

    // Check logs for workflow1
    let workflow1_logs = storage
        .get_logs_for_workflow("workflow1", Some(10))
        .unwrap();
    assert_eq!(workflow1_logs.len(), 3);

    // Check logs for workflow2
    let workflow2_logs = storage
        .get_logs_for_workflow("workflow2", Some(10))
        .unwrap();
    assert_eq!(workflow2_logs.len(), 3);

    // Check that messages are different
    assert!(workflow1_logs[0].message.contains("workflow1"));
    assert!(workflow2_logs[0].message.contains("workflow2"));
}

#[test]
fn test_api_response_caching() {
    let (storage, _temp_dir) = create_test_storage();

    let url = "https://api.example.com/test";
    let method = "GET";
    let headers = HashMap::from([("Authorization".to_string(), "Bearer token".to_string())]);
    let response_body = r#"{"status": "success"}"#;
    let status_code = 200;

    // Save API response
    let result = storage.save_api_response(
        url,
        method,
        &headers,
        None,
        response_body,
        status_code,
        Some(60), // 1 hour TTL
    );
    assert!(result.is_ok());

    // Retrieve cached response
    let cached = storage.get_cached_api_response(url, method, &headers, None);
    assert!(cached.is_ok());

    let cached_result = cached.unwrap();
    assert!(cached_result.is_some());

    let (cached_body, cached_status) = cached_result.unwrap();
    assert_eq!(cached_body, response_body);
    assert_eq!(cached_status, status_code);
}

#[test]
fn test_api_response_caching_with_body() {
    let (storage, _temp_dir) = create_test_storage();

    let url = "https://api.example.com/test";
    let method = "POST";
    let headers = HashMap::new();
    let request_body = Some(r#"{"key": "value"}"#);
    let response_body = r#"{"result": "created"}"#;
    let status_code = 201;

    // Save API response with request body
    storage
        .save_api_response(
            url,
            method,
            &headers,
            request_body,
            response_body,
            status_code,
            Some(30),
        )
        .unwrap();

    // Retrieve cached response - should match with same request body
    let cached = storage
        .get_cached_api_response(url, method, &headers, request_body)
        .unwrap();
    assert!(cached.is_some());

    // Try with different request body - should not match
    let different_body = Some(r#"{"key": "different"}"#);
    let cached_different = storage
        .get_cached_api_response(url, method, &headers, different_body)
        .unwrap();
    assert!(cached_different.is_none());
}

#[test]
fn test_api_response_cache_miss() {
    let (storage, _temp_dir) = create_test_storage();

    let url = "https://api.example.com/nonexistent";
    let method = "GET";
    let headers = HashMap::new();

    // Try to get a cached response that doesn't exist
    let cached = storage.get_cached_api_response(url, method, &headers, None);
    assert!(cached.is_ok());
    assert!(cached.unwrap().is_none());
}

#[test]
fn test_get_recent_workflow_runs_with_limit() {
    let (storage, _temp_dir) = create_test_storage();

    // Save multiple workflow runs
    for i in 0..5 {
        let run = WorkflowRun::new(&format!("workflow_{}", i));
        storage.save_workflow_run(&run).unwrap();
        // Small delay to ensure different timestamps for reliable ordering
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // Get recent runs with limit
    let recent_runs = storage.get_recent_workflow_runs(Some(3)).unwrap();
    assert!(recent_runs.len() <= 3); // Should be at most 3

    // Get all runs
    let all_runs = storage.get_recent_workflow_runs(None).unwrap();
    assert_eq!(all_runs.len(), 5);
}

#[test]
fn test_get_logs_with_limit() {
    let (storage, _temp_dir) = create_test_storage();

    // Save multiple log entries
    for i in 0..5 {
        let log_entry = LogEntry {
            timestamp: chrono::Utc::now(),
            level: LogLevel::Info,
            message: format!("Log message {}", i),
        };
        storage.save_log_entry("test_workflow", &log_entry).unwrap();
    }

    // Get logs with limit
    let limited_logs = storage
        .get_logs_for_workflow("test_workflow", Some(3))
        .unwrap();
    assert_eq!(limited_logs.len(), 3);

    // Get all logs
    let all_logs = storage
        .get_logs_for_workflow("test_workflow", None)
        .unwrap();
    assert_eq!(all_logs.len(), 5);
}

#[test]
fn test_nonexistent_workflow_logs() {
    let (storage, _temp_dir) = create_test_storage();

    // Try to get logs for a workflow that doesn't exist
    let logs = storage.get_logs_for_workflow("nonexistent_workflow", Some(10));
    assert!(logs.is_ok());
    assert!(logs.unwrap().is_empty());
}

#[test]
fn test_log_level_serialization() {
    // Test that log levels can be serialized and deserialized correctly
    let levels = [LogLevel::Info, LogLevel::Warning, LogLevel::Error];

    for level in levels {
        let serialized = serde_json::to_string(&level).unwrap();
        let deserialized: LogLevel = serde_json::from_str(&serialized).unwrap();
        assert_eq!(level, deserialized);
    }
}
