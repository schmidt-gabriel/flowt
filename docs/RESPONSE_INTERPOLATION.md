# Response Data Interpolation Feature

This document explains how to use response data from one step as input to another step in flowt workflows.

## Overview

The flowt workflow engine now supports template interpolation, allowing you to reference data from previous steps in your workflows. This is particularly useful for:

- Using HTTP response data in subsequent steps
- Passing shell command output to other nodes
- Building dynamic workflows based on previous results

## Syntax

Use the `${variable}` syntax for template interpolation:

### Step Response Data
```yaml
${steps.step-id.response.field}
${steps.step-id.response.nested.field}
${steps.step-id.response.arrayField.0}
```

### Step Metadata
```yaml
${steps.step-id.output}     # Raw output text
${steps.step-id.status}     # Step status (Success, Failed, etc.)
```

### Environment Variables
```yaml
${ENVIRONMENT_VARIABLE}
```

## Examples

### HTTP Response Data
```yaml
nodes:
  - id: fetch-user
    type: http
    url: "https://jsonplaceholder.typicode.com/users/1"
    method: GET
    expect_status: 200
    
  - id: greet-user
    type: shell
    cmd: "echo 'Hello ${steps.fetch-user.response.name}!'"
    depends_on: ["fetch-user"]
    
```

### Nested JSON Fields
```yaml
nodes:
  - id: fetch-config
    type: http
    url: "https://api.example.com/config"
    method: GET
    
  - id: deploy
    type: shell
    cmd: "kubectl apply -f ${steps.fetch-config.response.deployment.manifest_path}"
    env:
      NAMESPACE: "${steps.fetch-config.response.deployment.namespace}"
    depends_on: ["fetch-config"]
```

### Array Access
```yaml
nodes:
  - id: fetch-items
    type: http
    url: "https://api.example.com/items"
    method: GET
    
  - id: process-first-item
    type: shell
    cmd: "echo 'Processing: ${steps.fetch-items.response.0.name}'"
    depends_on: ["fetch-items"]
```

## Supported Node Types

Template interpolation works in all node types:

### HTTP Nodes
- `url`
- `headers` (values)
- `body`

### Shell Nodes
- `cmd`
- `env` (values)

### Log Nodes
- `message`

## Error Handling

If a referenced field doesn't exist, the template variable will remain unchanged:
- `${steps.missing-step.response.field}` → `${steps.missing-step.response.field}`

For missing environment variables:
- `${MISSING_VAR}` → `${MISSING_VAR}`
