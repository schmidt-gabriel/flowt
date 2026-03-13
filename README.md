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

### Manual Install
1. Go to the GitHub [releases](https://github.com/schmidt-gabriel/flowt/releases/latest) page
2. Download the latest non-dSYM build (i.e., `flowt-X.Y.Z.tar.gz` for Linux/macOS or `flowt-X.Y.Z.zip` for Windows)
3. Unzip the archive
4. Run the `flowt` binary from the terminal

### Install via Homebrew :beer:
1. Install [Homebrew](https://brew.sh) if you haven't already
2. Open Terminal and run `brew tap schmidt-gabriel/tap`
3. Run `brew install flowt`

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
flowt run workflows/simple_workflow.yaml

# Launch the TUI dashboard explicitly
flowt tui [workflows-directory]

# Start the workflow service in terminal mode (logs only)
flowt serve [workflows-directory]

# List workflows in a directory
flowt list [workflows-directory]
```

### TUI Dashboard

Launch the terminal user interface with smart mode detection:

```bash
# Default TUI launch (smart mode: full engine OR UI-only based on service status)
flowt

# TUI with custom directory
flowt -d /path/to/workflows
# or
flowt tui /path/to/workflows
```

**Smart Behavior:**
- **No service running**: TUI acts as full engine with cron scheduling
- **Service detected**: TUI connects as UI-only interface to existing service

The TUI provides a real-time view of running workflows, their status, execution history, and shows the current mode in the status header.

### Service Mode

Flowt supports two deployment patterns, with automatic detection:

#### 1. Standalone TUI (Full Engine)
When no service is running, TUI acts as a complete workflow engine:

```bash
flowt          # TUI with full engine - runs cron jobs and automation
```

**Features:**
- **Full Engine**: Cron scheduler, workflow automation, and UI in one process
- **Simple Setup**: Everything in a single command
- **Complete Monitoring**: Workflows, logs, and scheduling all managed together

#### 2. Service + TUI (Distributed Mode)
Run as a persistent service with multiple UI connections:

```bash
# Terminal 1: Start the persistent service
flowt serve [workflows-directory]

# Terminal 2+: Connect TUI(s) to the running service  
flowt tui [workflows-directory]  # Automatically detects and connects
```

**Features:**
- **Persistent Engine**: Cron jobs and workflows continue running in the background. Note: on startup, the service will automatically run all manual-trigger workflows.
- **Multiple TUI Connections**: Connect multiple TUI instances to the same service
- **Terminal Logs**: Real-time colored logs in the service terminal
- **Shared State**: All TUI connections share the same workflow runs and history
- **Graceful Shutdown**: Use Ctrl+C to stop the service cleanly

#### Smart Mode Detection

The TUI automatically detects which mode to use:
- **▶ Connected to Flowt service**: When `flowt serve` is running, TUI connects as UI-only
- **▶ TUI Engine Mode**: When no service is running, TUI starts full engine with cron scheduling

This prevents duplicate cron jobs while ensuring automation is always available.

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

#### Explicit Dependencies
```yaml
- id: deploy
  type: shell
  cmd: "echo 'Deploying'"
  depends_on:
    - build
    - test
```

#### Disabling Workflows
You can disable a workflow by adding `enabled: false` to the top level of the workflow file.
```yaml
name: my-workflow
enabled: false
...
```

#### Response Interpolation
You can interpolate values from the output of previous steps. For more details, see the [Response Interpolation documentation](docs/RESPONSE_INTERPOLATION.md).

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

- **simple_workflow.yaml**: Simple service health check workflow

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
