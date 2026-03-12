use flowt::engine::Engine;
use serde_json::json;
use std::collections::HashMap;

// Test the template interpolation functionality
#[test]
fn test_template_interpolation_basic() {
    let engine = Engine::new();

    // Create mock context with node results
    let mut context = HashMap::new();
    let node_result = flowt::engine::NodeResult {
        node_id: "test_node".to_string(),
        status: flowt::engine::NodeStatus::Success,
        output: "test output".to_string(),
        response_data: Some(json!({"user": {"name": "John", "id": 123}})),
        started_at: chrono::Utc::now(),
        finished_at: Some(chrono::Utc::now()),
    };
    context.insert("test_node".to_string(), node_result);

    // Test environment variable interpolation
    std::env::set_var("TEST_ENV_VAR", "test_value");
    let result = engine.interpolate_template("Hello ${TEST_ENV_VAR}", &context);
    assert_eq!(result, "Hello test_value");

    // Test step output interpolation
    let result = engine.interpolate_template("Output: ${steps.test_node.output}", &context);
    assert_eq!(result, "Output: test output");

    // Test step status interpolation
    let result = engine.interpolate_template("Status: ${steps.test_node.status}", &context);
    assert_eq!(result, "Status: Success");

    // Test response data interpolation
    let result =
        engine.interpolate_template("User: ${steps.test_node.response.user.name}", &context);
    assert_eq!(result, "User: John");

    let result = engine.interpolate_template("ID: ${steps.test_node.response.user.id}", &context);
    assert_eq!(result, "ID: 123");
}

#[test]
fn test_template_interpolation_missing_values() {
    let engine = Engine::new();
    let context = HashMap::new();

    // Test missing environment variable
    let result = engine.interpolate_template("Value: ${MISSING_VAR}", &context);
    assert_eq!(result, "Value: ${MISSING_VAR}");

    // Test missing step
    let result = engine.interpolate_template("Output: ${steps.missing_step.output}", &context);
    assert_eq!(result, "Output: ${steps.missing_step.output}");
}

#[test]
fn test_template_interpolation_complex() {
    let engine = Engine::new();

    let mut context = HashMap::new();
    let api_result = flowt::engine::NodeResult {
        node_id: "api_call".to_string(),
        status: flowt::engine::NodeStatus::Success,
        output: "HTTP 200".to_string(),
        response_data: Some(json!({
            "data": {
                "items": [
                    {"name": "item1", "count": 5},
                    {"name": "item2", "count": 10}
                ],
                "total": 15
            }
        })),
        started_at: chrono::Utc::now(),
        finished_at: Some(chrono::Utc::now()),
    };
    context.insert("api_call".to_string(), api_result);

    // Test complex interpolation with multiple variables
    let template = "API returned ${steps.api_call.output} with ${steps.api_call.response.data.total} total items";
    let result = engine.interpolate_template(template, &context);
    assert_eq!(result, "API returned HTTP 200 with 15 total items");

    // Test array access
    let template = "First item: ${steps.api_call.response.data.items.0.name} (${steps.api_call.response.data.items.0.count})";
    let result = engine.interpolate_template(template, &context);
    assert_eq!(result, "First item: item1 (5)");
}

#[test]
fn test_interpolate_headers() {
    let engine = Engine::new();

    let mut context = HashMap::new();
    let auth_result = flowt::engine::NodeResult {
        node_id: "auth".to_string(),
        status: flowt::engine::NodeStatus::Success,
        output: "authenticated".to_string(),
        response_data: Some(json!({"token": "abc123"})),
        started_at: chrono::Utc::now(),
        finished_at: Some(chrono::Utc::now()),
    };
    context.insert("auth".to_string(), auth_result);

    let mut headers = HashMap::new();
    headers.insert(
        "Authorization".to_string(),
        "Bearer ${steps.auth.response.token}".to_string(),
    );
    headers.insert("Content-Type".to_string(), "application/json".to_string());

    let interpolated = engine.interpolate_headers(&headers, &context);

    assert_eq!(
        interpolated.get("Authorization"),
        Some(&"Bearer abc123".to_string())
    );
    assert_eq!(
        interpolated.get("Content-Type"),
        Some(&"application/json".to_string())
    );
}

#[test]
fn test_interpolate_env() {
    let engine = Engine::new();

    let mut context = HashMap::new();
    let setup_result = flowt::engine::NodeResult {
        node_id: "setup".to_string(),
        status: flowt::engine::NodeStatus::Success,
        output: "/tmp/workdir".to_string(),
        response_data: None,
        started_at: chrono::Utc::now(),
        finished_at: Some(chrono::Utc::now()),
    };
    context.insert("setup".to_string(), setup_result);

    std::env::set_var("HOME", "/home/user");

    let mut env = HashMap::new();
    env.insert("WORK_DIR".to_string(), "${steps.setup.output}".to_string());
    env.insert("CONFIG_PATH".to_string(), "${HOME}/.config".to_string());
    env.insert("STATIC_VAR".to_string(), "static_value".to_string());

    let interpolated = engine.interpolate_env(&env, &context);

    assert_eq!(
        interpolated.get("WORK_DIR"),
        Some(&"/tmp/workdir".to_string())
    );
    assert_eq!(
        interpolated.get("CONFIG_PATH"),
        Some(&"/home/user/.config".to_string())
    );
    assert_eq!(
        interpolated.get("STATIC_VAR"),
        Some(&"static_value".to_string())
    );
}

#[test]
fn test_nested_value_retrieval() {
    let engine = Engine::new();

    let json_data = json!({
        "level1": {
            "level2": {
                "level3": "deep_value"
            },
            "array": [
                {"name": "first"},
                {"name": "second"}
            ]
        },
        "simple": "simple_value"
    });

    // Test deep nested access
    let value = engine.get_nested_value(&json_data, &["level1", "level2", "level3"]);
    assert!(value.is_some());
    assert_eq!(engine.value_to_string(&value.unwrap()), "deep_value");

    // Test array access
    let value = engine.get_nested_value(&json_data, &["level1", "array", "0", "name"]);
    assert!(value.is_some());
    assert_eq!(engine.value_to_string(&value.unwrap()), "first");

    // Test simple access
    let value = engine.get_nested_value(&json_data, &["simple"]);
    assert!(value.is_some());
    assert_eq!(engine.value_to_string(&value.unwrap()), "simple_value");

    // Test missing path
    let value = engine.get_nested_value(&json_data, &["missing", "path"]);
    assert!(value.is_none());

    // Test invalid array index
    let value = engine.get_nested_value(&json_data, &["level1", "array", "10", "name"]);
    assert!(value.is_none());
}

#[test]
fn test_value_to_string_conversion() {
    let engine = Engine::new();

    // Test string
    let value = json!("test string");
    assert_eq!(engine.value_to_string(&value), "test string");

    // Test number
    let value = json!(42);
    assert_eq!(engine.value_to_string(&value), "42");

    // Test boolean
    let value = json!(true);
    assert_eq!(engine.value_to_string(&value), "true");

    // Test null
    let value = json!(null);
    assert_eq!(engine.value_to_string(&value), "null");

    // Test complex object
    let value = json!({"key": "value", "nested": {"inner": 123}});
    let result = engine.value_to_string(&value);
    // Should be valid JSON string
    assert!(result.contains("key"));
    assert!(result.contains("value"));
}

#[test]
fn test_multiple_interpolations_in_template() {
    let engine = Engine::new();

    std::env::set_var("SERVICE_URL", "https://api.example.com");
    std::env::set_var("API_VERSION", "v1");

    let mut context = HashMap::new();
    let result1 = flowt::engine::NodeResult {
        node_id: "step1".to_string(),
        status: flowt::engine::NodeStatus::Success,
        output: "success".to_string(),
        response_data: Some(json!({"id": "12345"})),
        started_at: chrono::Utc::now(),
        finished_at: Some(chrono::Utc::now()),
    };
    context.insert("step1".to_string(), result1);

    let template = "${SERVICE_URL}/${API_VERSION}/users/${steps.step1.response.id}?status=${steps.step1.output}";
    let result = engine.interpolate_template(template, &context);
    assert_eq!(
        result,
        "https://api.example.com/v1/users/12345?status=success"
    );
}

#[test]
fn test_interpolation_with_special_characters() {
    let engine = Engine::new();

    let mut context = HashMap::new();
    let node_result = flowt::engine::NodeResult {
        node_id: "special_test".to_string(),
        status: flowt::engine::NodeStatus::Success,
        output: "output with spaces and symbols!@#$%".to_string(),
        response_data: Some(json!({"message": "Hello, World! How are you?"})),
        started_at: chrono::Utc::now(),
        finished_at: Some(chrono::Utc::now()),
    };
    context.insert("special_test".to_string(), node_result);

    let template =
        "Message: ${steps.special_test.response.message} | Output: ${steps.special_test.output}";
    let result = engine.interpolate_template(template, &context);
    assert_eq!(
        result,
        "Message: Hello, World! How are you? | Output: output with spaces and symbols!@#$%"
    );
}
