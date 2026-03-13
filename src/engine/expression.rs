use std::collections::HashMap;

use crate::engine::{NodeResult, NodeStatus};

#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
    Equals,
    NotEquals,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Term {
    Variable(String),
    Value(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    pub left: Term,
    pub op: Operator,
    pub right: Term,
}

impl Expression {
    pub fn parse(input: &str) -> Option<Self> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.len() != 3 {
            return None;
        }

        let left = Term::Variable(parts[0].to_string());

        let op = match parts[1] {
            "==" => Operator::Equals,
            "!=" => Operator::NotEquals,
            _ => return None,
        };

        let right = Term::Value(parts[2].to_string());

        Some(Expression { left, op, right })
    }

    pub fn evaluate(&self, context: &HashMap<String, NodeResult>) -> bool {
        let left_val = match &self.left {
            Term::Variable(var) => {
                let var_parts: Vec<&str> = var.split('.').collect();
                let step_id = if var_parts.len() == 2 && var_parts[0] == "steps" {
                    var_parts[1]
                } else {
                    var_parts[0]
                };

                context.get(step_id).map(|result| {
                    let status_str = match result.status {
                        NodeStatus::Success => "success",
                        NodeStatus::Failed(_) => "failed",
                        NodeStatus::Skipped => "skipped",
                        NodeStatus::Running => "running",
                        NodeStatus::Pending => "pending",
                    };
                    status_str.to_string()
                })
            }
            Term::Value(val) => Some(val.clone()),
        };

        let right_val = match &self.right {
            Term::Variable(_) => {
                // For now, right side can only be a value
                None
            }
            Term::Value(val) => Some(val.clone()),
        };

        match (left_val, right_val) {
            (Some(l), Some(r)) => match self.op {
                Operator::Equals => l.eq_ignore_ascii_case(&r),
                Operator::NotEquals => !l.eq_ignore_ascii_case(&r),
            },
            _ => false,
        }
    }
}
