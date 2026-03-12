use crate::engine::{WorkflowRun, NodeStatus};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub fn run_details(run: &WorkflowRun) -> Vec<Line<'static>> {
    let mut lines = vec![];

    lines.push(Line::from(vec![
        Span::styled("Workflow: ", Style::default().fg(Color::DarkGray)),
        Span::styled(run.workflow_name.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Run ID:   ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("#{}", run.id), Style::default().fg(Color::Cyan)),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Started:  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            run.started_at.format("%H:%M:%S").to_string(),
            Style::default().fg(Color::White),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Execution Graph:", Style::default().fg(Color::DarkGray))));
    lines.push(Line::from(""));

    // Draw the execution graph showing flow between nodes in order
    for (index, result) in run.node_results.iter().enumerate() {
        // Add connector line for execution flow (except for first node)
        if index > 0 {
            lines.push(Line::from(vec![
                Span::styled("      │", Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("      ↓", Style::default().fg(Color::DarkGray)),
            ]));
        }

        let (icon, color) = match &result.status {
            NodeStatus::Pending => ("○", Color::DarkGray),
            NodeStatus::Running => ("⟳", Color::Yellow),
            NodeStatus::Success => ("✔", Color::Green),
            NodeStatus::Failed(_) => ("✘", Color::Red),
            NodeStatus::Skipped => ("-", Color::DarkGray),
        };

        // Add execution order number and node info
        let order_num = format!("{:2}.", index + 1);
        lines.push(Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled("┌─", Style::default().fg(Color::DarkGray)),
            Span::styled(order_num, Style::default().fg(Color::Cyan)),
            Span::styled(" ", Style::default()),
            Span::styled(format!("{} ", icon), Style::default().fg(color)),
            Span::styled(result.node_id.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]));

        // Show execution timing if available
        if let Some(finished_at) = result.finished_at {
            let duration = finished_at - result.started_at;
            let duration_ms = duration.num_milliseconds();
            lines.push(Line::from(vec![
                Span::styled(" │  ├─ ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("Duration: {}ms", duration_ms), Style::default().fg(Color::DarkGray)),
            ]));
        }

        // Show status info
        match &result.status {
            NodeStatus::Running => {
                lines.push(Line::from(vec![
                    Span::styled(" │  ├─ ", Style::default().fg(Color::Yellow)),
                    Span::styled("Status: Running...", Style::default().fg(Color::Yellow)),
                ]));
            },
            NodeStatus::Skipped => {
                lines.push(Line::from(vec![
                    Span::styled(" │  ├─ ", Style::default().fg(Color::DarkGray)),
                    Span::styled("Skipped: Dependencies not met", Style::default().fg(Color::DarkGray)),
                ]));
            },
            _ => {}
        }

        // Show truncated output if available
        if !result.output.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(" │  ├─ ", Style::default().fg(Color::DarkGray)),
                Span::styled("Output:", Style::default().fg(Color::DarkGray)),
            ]));
            for (line_idx, line) in result.output.lines().take(2).enumerate() {
                let connector = if line_idx == 1 && result.output.lines().count() > 2 {
                    " │  │   └─ [...]"
                } else {
                    " │  │      "
                };
                let output_line = if line_idx == 1 && result.output.lines().count() > 2 {
                    format!("{}{}", connector, "")
                } else {
                    format!("{}{}", connector, line)
                };
                lines.push(Line::from(vec![Span::styled(
                    output_line,
                    Style::default().fg(Color::DarkGray),
                )]));
            }
        }

        // Show error details
        if let NodeStatus::Failed(err) = &result.status {
            lines.push(Line::from(vec![
                Span::styled(" │  └─ ", Style::default().fg(Color::Red)),
                Span::styled(format!("Error: {}", err), Style::default().fg(Color::Red)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(" │", Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    // Add summary at the bottom
    if !run.node_results.is_empty() {
        let total_nodes = run.node_results.len();
        let completed_nodes = run.node_results.iter().filter(|r| !matches!(r.status, NodeStatus::Pending | NodeStatus::Running)).count();
        let successful_nodes = run.node_results.iter().filter(|r| matches!(r.status, NodeStatus::Success)).count();
        let failed_nodes = run.node_results.iter().filter(|r| matches!(r.status, NodeStatus::Failed(_))).count();
        let skipped_nodes = run.node_results.iter().filter(|r| matches!(r.status, NodeStatus::Skipped)).count();
        
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(" └─ ", Style::default().fg(Color::DarkGray)),
            Span::styled("Graph Summary:", Style::default().fg(Color::DarkGray)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("    Progress: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}/{} nodes completed", completed_nodes, total_nodes), Style::default().fg(Color::White)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("    Results:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} ✔", successful_nodes), Style::default().fg(Color::Green)),
            Span::styled(format!(" · {} ✘", failed_nodes), Style::default().fg(Color::Red)),
            Span::styled(format!(" · {} -", skipped_nodes), Style::default().fg(Color::DarkGray)),
        ]));
    }

    lines
}

pub fn format_duration(duration: chrono::Duration) -> String {
    let total_seconds = duration.num_seconds();
    
    if total_seconds < 0 {
        return "overdue".to_string();
    }
    
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    
    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}
