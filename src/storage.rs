use anyhow::Result;
use chrono::{DateTime, Utc};
use polodb_core::Database;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::engine::{NodeResult, RunStatus, WorkflowRun};
use crate::tui::{LogEntry, LogLevel};

pub struct StorageService {
    db: Database,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PersistedWorkflowRun {
    #[serde(rename = "_id")]
    pub id: String,
    pub workflow_name: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub node_results: Vec<NodeResult>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PersistedLogEntry {
    #[serde(rename = "_id")]
    pub id: String,
    pub workflow_name: String,
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ApiResponse {
    #[serde(rename = "_id")]
    pub id: String,
    pub url: String,
    pub method: String,
    pub headers_hash: String, // Changed from u64 to String to avoid BSON issues
    pub body_hash: Option<String>, // Changed from Option<u64> to Option<String>
    pub response_data: String,
    pub status_code: u16,
    pub timestamp: DateTime<Utc>,
    pub ttl_minutes: Option<u32>, // TTL for cache invalidation
}

impl StorageService {
    pub fn new() -> Result<Self> {
        let db_path = Self::get_db_path()?;
        Self::new_with_path(&db_path)
    }

    pub fn new_with_path<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = Database::open_file(&db_path)?;
        Ok(Self { db })
    }

    fn get_db_path() -> Result<PathBuf> {
        let db_path = std::env::var("FLOWT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .map(|mut home| {
                        home.push(".flowt");
                        home
                    })
                    .unwrap_or_else(|| PathBuf::from("."))
            });
        Ok(db_path.join("flowt_storage.db"))
    }

    // Workflow Run Operations
    pub fn save_workflow_run(&self, run: &WorkflowRun) -> Result<()> {
        let collection = self.db.collection::<PersistedWorkflowRun>("workflow_runs");
        let persisted_run = PersistedWorkflowRun {
            id: run.id.clone(),
            workflow_name: run.workflow_name.clone(),
            status: run.status.clone(),
            started_at: run.started_at,
            finished_at: run.finished_at,
            node_results: run.node_results.clone(),
        };

        collection.insert_one(&persisted_run)?;
        Ok(())
    }

    pub fn update_workflow_run(&self, run: &WorkflowRun) -> Result<()> {
        let collection = self.db.collection::<PersistedWorkflowRun>("workflow_runs");
        let persisted_run = PersistedWorkflowRun {
            id: run.id.clone(),
            workflow_name: run.workflow_name.clone(),
            status: run.status.clone(),
            started_at: run.started_at,
            finished_at: run.finished_at,
            node_results: run.node_results.clone(),
        };

        // Delete the existing document and insert the new one
        let query = polodb_core::bson::doc! { "_id": &run.id };
        collection.delete_one(query)?;
        collection.insert_one(&persisted_run)?;
        Ok(())
    }

    pub fn get_recent_workflow_runs(&self, limit: Option<usize>) -> Result<Vec<WorkflowRun>> {
        let collection = self.db.collection::<PersistedWorkflowRun>("workflow_runs");

        let mut runs = Vec::new();
        let cursor = collection.find(None)?;

        for doc in cursor {
            let persisted_run = doc?;
            runs.push(WorkflowRun {
                id: persisted_run.id,
                workflow_name: persisted_run.workflow_name,
                status: persisted_run.status,
                started_at: persisted_run.started_at,
                finished_at: persisted_run.finished_at,
                node_results: persisted_run.node_results,
            });
        }

        // Sort by started_at descending (most recent first)
        runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        if let Some(limit_val) = limit {
            runs.truncate(limit_val);
        }

        Ok(runs)
    }

    // Log Operations
    pub fn save_log_entry(&self, workflow_name: &str, entry: &LogEntry) -> Result<()> {
        let collection = self.db.collection::<PersistedLogEntry>("logs");
        let persisted_log = PersistedLogEntry {
            id: format!(
                "{}_{}",
                entry.timestamp.timestamp_nanos_opt().unwrap_or(0),
                workflow_name
            ),
            workflow_name: workflow_name.to_string(),
            timestamp: entry.timestamp,
            level: entry.level.clone(),
            message: entry.message.clone(),
        };

        collection.insert_one(&persisted_log)?;
        Ok(())
    }

    pub fn get_logs_for_workflow(
        &self,
        workflow_name: &str,
        limit: Option<usize>,
    ) -> Result<Vec<LogEntry>> {
        let collection = self.db.collection::<PersistedLogEntry>("logs");

        let query = polodb_core::bson::doc! { "workflow_name": workflow_name };
        let mut logs = Vec::new();
        let cursor = collection.find(Some(query))?;

        for doc in cursor {
            let persisted_log = doc?;
            logs.push(LogEntry {
                timestamp: persisted_log.timestamp,
                level: persisted_log.level,
                message: persisted_log.message,
            });
        }

        // Sort by timestamp ascending (oldest first, like current implementation)
        logs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        if let Some(limit_val) = limit {
            logs.truncate(limit_val);
        }

        Ok(logs)
    }

    // API Response Cache Operations
    fn generate_cache_key(
        url: &str,
        method: &str,
        headers: &std::collections::HashMap<String, String>,
        body: Option<&str>,
    ) -> String {
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        method.hash(&mut hasher);

        let mut sorted_headers: Vec<_> = headers.iter().collect();
        sorted_headers.sort_by_key(|(k, _)| *k);
        for (k, v) in sorted_headers {
            k.hash(&mut hasher);
            v.hash(&mut hasher);
        }

        if let Some(body_content) = body {
            body_content.hash(&mut hasher);
        }

        format!("api_{}_{}", method.to_lowercase(), hasher.finish())
    }

    pub fn save_api_response(
        &self,
        url: &str,
        method: &str,
        headers: &std::collections::HashMap<String, String>,
        body: Option<&str>,
        response_data: &str,
        status_code: u16,
        ttl_minutes: Option<u32>,
    ) -> Result<()> {
        let collection = self.db.collection::<ApiResponse>("api_cache");

        let cache_key = Self::generate_cache_key(url, method, headers, body);

        let mut headers_hasher = DefaultHasher::new();
        let mut sorted_headers: Vec<_> = headers.iter().collect();
        sorted_headers.sort_by_key(|(k, _)| *k);
        for (k, v) in sorted_headers {
            k.hash(&mut headers_hasher);
            v.hash(&mut headers_hasher);
        }
        let headers_hash = headers_hasher.finish().to_string();

        let body_hash = body.map(|b| {
            let mut body_hasher = DefaultHasher::new();
            b.hash(&mut body_hasher);
            body_hasher.finish().to_string()
        });

        let api_response = ApiResponse {
            id: cache_key.clone(),
            url: url.to_string(),
            method: method.to_string(),
            headers_hash,
            body_hash,
            response_data: response_data.to_string(),
            status_code,
            timestamp: Utc::now(),
            ttl_minutes,
        };

        // Remove existing entry if any
        let query = polodb_core::bson::doc! { "_id": &cache_key };
        let _ = collection.delete_one(query);

        collection.insert_one(&api_response)?;
        Ok(())
    }

    pub fn get_cached_api_response(
        &self,
        url: &str,
        method: &str,
        headers: &std::collections::HashMap<String, String>,
        body: Option<&str>,
    ) -> Result<Option<(String, u16)>> {
        let collection = self.db.collection::<ApiResponse>("api_cache");

        let cache_key = Self::generate_cache_key(url, method, headers, body);
        let query = polodb_core::bson::doc! { "_id": &cache_key };

        if let Ok(Some(cached_response)) = collection.find_one(query) {
            // Check if cache is still valid (TTL)
            if let Some(ttl) = cached_response.ttl_minutes {
                let age_minutes = (Utc::now() - cached_response.timestamp).num_minutes() as u32;
                if age_minutes > ttl {
                    // Cache expired, remove it
                    let delete_query = polodb_core::bson::doc! { "_id": &cache_key };
                    let _ = collection.delete_one(delete_query);
                    return Ok(None);
                }
            }

            Ok(Some((
                cached_response.response_data,
                cached_response.status_code,
            )))
        } else {
            Ok(None)
        }
    }
}
