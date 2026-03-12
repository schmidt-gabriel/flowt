<div align="center">
  <img src="https://github.com/user-attachments/assets/d2e7cf57-369d-4249-936d-35197e98fdd9" alt="Flowt Logo" width="200"/>
</div>


# Flowt


> **⚠️ Attention: This project is still in development. Features and APIs may change at any time. Use with caution.**

A powerful workflow automation engine built in Rust. Define workflows as YAML files and execute them with a beautiful terminal interface.

<p align="center">
  <img src="https://github.com/user-attachments/assets/6de8e05d-c098-4709-8cae-2a61e7317c7d" alt="Flowt Demo GIF" width="800"/>
</p>

## Features

- **Fast & Lightweight**: Built in Rust for maximum performance
- **YAML Configuration**: Simple and readable workflow definitions
- **Terminal UI**: Beautiful TUI dashboard for monitoring workflows
- **Multiple Triggers**: Manual, cron-based triggers
- **HTTP Requests**: Make API calls with custom headers and bodies
- **Shell Commands**: Execute shell scripts seamlessly
- **Logging**: Built-in logging for workflow monitoring
- **Conditional Execution**: Run nodes based on conditions
- **Retry Logic**: Automatic retries for failed operations
- **Timeouts**: Configurable timeouts for long-running tasks

## Installation

### From Source

```bash
git clone https://github.com/schmidt-gabriel/flowt.git
cd flowt
cargo build --release
./target/release/flowt --help
```

### Using Cargo

```bash
cargo install --path .
```

## Usage

### Commands

```bash
# Launch the TUI dashboard (default)
flowt
# or specify a custom workflows directory
flowt -d /path/to/workflows

# Run a workflow file directly
flowt run workflows/health-check.yaml

# Launch the TUI dashboard explicitly
flowt tui [workflows-directory]

# List workflows in a directory
flowt list [workflows-directory]
```

### TUI Dashboard

Launch the terminal user interface to monitor and manage workflows. This is the default mode:

```bash
# Default TUI launch
flowt

# TUI with custom directory
flowt -d /path/to/workflows
# or
flowt tui /path/to/workflows
```

The TUI provides a real-time view of running workflows, their status, and execution history.

## Workflow Configuration

Workflows are defined using YAML files with the following structure:

```yaml
name: my-workflow
description: Description of what this workflow does

triggers:
  - type: manual                    # Manual trigger
  # - type: cron                    # Scheduled trigger
  #   schedule: "0 */6 * * *"       # Every 6 hours

nodes:
  - id: step-1
    type: shell
    cmd: "echo 'Starting workflow'"
    
  - id: step-2
    type: http
    url: "https://api.example.com/health"
    method: GET
    expect_status: 200
    retry: 3
    timeout: "30s"
    
  - id: step-3
    type: log
    message: "Workflow finished"
```

### Node Types

#### HTTP Requests
```yaml
- id: api-call
  type: http
  url: "https://api.example.com/data"
  method: POST
  headers:
    Content-Type: "application/json"
    Authorization: "Bearer ${API_TOKEN}"
  body: '{"key": "value"}'
  expect_status: 201
```

#### Shell Commands
```yaml
- id: build-project
  type: shell
  cmd: "npm run build"
  env:
    NODE_ENV: "production"
```

#### Logging
```yaml
- id: log-status
  type: log
  message: "Checkpoint reached at $(date)"
```

### Advanced Features

#### Conditional Execution
```yaml
- id: cleanup
  type: shell
  cmd: "rm -rf temp/"
  when: "build == success"  # Only run if 'build' node succeeded
```

#### Retries and Timeouts
```yaml
- id: flaky-service
  type: http
  url: "https://unreliable-api.com"
  retry: 3           # Retry up to 3 times
  timeout: "60s"     # Timeout after 60 seconds
```

#### Environment Variables
Use environment variables in your workflows:
```yaml
- id: deploy
  type: shell
  cmd: "kubectl apply -f ${MANIFEST_PATH}"
  env:
    KUBECONFIG: "${HOME}/.kube/config"
```

## Directory Structure

```
flowt/
├── src/
│   ├── main.rs      # CLI entry point
│   ├── config.rs    # Workflow configuration
│   ├── engine/      # Workflow execution engine
│   └── tui/         # Terminal user interface
├── workflows/       # Example workflow files
└── Cargo.toml       # Rust dependencies
```

## Configuration

### Directory Configuration

Flowt uses configurable directories for workflows and internal storage. You can customize these locations using environment variables or command-line arguments.

#### Environment Variables

Flowt supports the following environment variables for directory configuration:

| Variable | Description | Default |
|----------|-------------|---------|
| `FLOWT_DIR` | Base directory for all Flowt data | `~/.flowt` |

#### Examples

```bash
# Use default locations
flowt

# Set base directory
export FLOWT_DIR="/my/flowt"
flowt

# Override with command line
flowt --dir "/another/path"
```

### Workflow Directory

You can specify a different workflows directory using the command line:

```bash
flowt tui /path/to/my/workflows
flowt list /path/to/my/workflows
```

### Environment Variables

Flowt supports environment variable substitution in workflow files using the `${VARIABLE_NAME}` syntax.

## Examples

Check the `workflows/` directory for example workflow configurations:

- **health-check.yaml**: Simple service health check workflow

## Development

### Prerequisites

- Rust 1.70+ 
- Cargo

### Building

```bash
cargo build
```

### Running Tests

```bash
cargo test
```

### Running in Development

```bash
cargo run -- tui
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- Built with [Ratatui](https://github.com/ratatui-org/ratatui) for the terminal interface
- Uses [Tokio](https://tokio.rs) for async runtime
- CLI powered by [Clap](https://github.com/clap-rs/clap)
- Storage powered by [PoloDb](https://github.com/polodb/polodb)

---

Happy automating!
