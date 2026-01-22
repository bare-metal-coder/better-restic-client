use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{Config, BackupConfig, ResticConfig};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    pub config_path: String,
    pub log_dir: std::path::PathBuf,
}

#[derive(Deserialize)]
pub struct UpdateYamlRequest {
    pub yaml: String,
}

#[derive(Deserialize, Serialize)]
pub struct BackupRequest {
    pub dry_run: Option<bool>,
}

pub async fn run_web_server(state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/", get(index))
        .route("/api/config", get(get_config))
        .route("/api/logs", get(get_logs))
        .route("/api/status", get(get_status))
        .route("/api/snapshots", get(get_snapshots))
        .route("/api/config/yaml", get(get_config_yaml))
        .route("/api/config/yaml", post(update_config_yaml))
        .route("/api/backup/trigger", post(trigger_backup))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("🌐 Web UI available at http://127.0.0.1:3000");
    println!("   Press Ctrl+C to stop the server");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn get_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.config.read().await;
    Json(json!({
        "backup": {
            "frequency": config.backup.frequency,
            "time": config.backup.time,
            "directories": config.backup.directories.iter().map(|d: &PathBuf| d.to_string_lossy().to_string()).collect::<Vec<_>>(),
            "exclude": config.backup.exclude.iter().map(|d: &PathBuf| d.to_string_lossy().to_string()).collect::<Vec<_>>(),
        },
        "logging": {
            "directory": config.logging.directory.to_string_lossy(),
            "max_size": config.logging.max_size,
        },
        "restic": {
            "repository": config.restic.repository,
            "has_ssh_command": config.restic.ssh_command.is_some(),
            "has_password_command": config.restic.password_command.is_some(),
            "has_password": config.restic.password.is_some(),
        }
    }))
}

async fn get_config_yaml(State(state): State<AppState>) -> Result<String, StatusCode> {
    std::fs::read_to_string(&state.config_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn update_config_yaml(
    State(state): State<AppState>,
    Json(payload): Json<UpdateYamlRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Validate YAML by trying to parse it
    let new_config: Config = serde_yaml::from_str(&payload.yaml)
        .map_err(|e| {
            eprintln!("YAML validation error: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    // Write to file
    std::fs::write(&state.config_path, &payload.yaml)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Update in-memory config
    *state.config.write().await = new_config;

    Ok(Json(json!({
        "success": true,
        "message": "Configuration updated successfully"
    })))
}

async fn trigger_backup(
    State(state): State<AppState>,
    Json(payload): Json<BackupRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let dry_run = payload.dry_run.unwrap_or(false);
    let config = state.config.read().await;
    
    // Spawn backup in background
    let backup_config = BackupConfig {
        frequency: config.backup.frequency.clone(),
        time: config.backup.time.clone(),
        directories: config.backup.directories.clone(),
        exclude: config.backup.exclude.clone(),
    };
    
    let restic_config = ResticConfig {
        repository: config.restic.repository.clone(),
        ssh_command: config.restic.ssh_command.clone(),
        password_command: config.restic.password_command.clone(),
        password: config.restic.password.clone(),
    };
    
    drop(config); // Release the lock
    
    // Execute backup in a tokio task
    tokio::spawn(async move {
        if let Err(e) = crate::execute_restic_backup(&backup_config, &restic_config, dry_run, true).await {
            eprintln!("Backup error: {}", e);
        }
    });
    
    Ok(Json(json!({
        "success": true,
        "message": if dry_run { "Dry run backup triggered" } else { "Backup triggered" },
        "dry_run": dry_run
    })))
}

async fn get_logs(State(state): State<AppState>) -> Json<serde_json::Value> {
    let log_dir = &state.log_dir;
    let mut log_files = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("restic_backup") {
                    if let Ok(metadata) = entry.metadata() {
                        log_files.push(json!({
                            "name": name,
                            "size": metadata.len(),
                            "modified": metadata.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())),
                        }));
                    }
                }
            }
        }
    }
    
    // Sort by modified time (newest first)
    log_files.sort_by(|a, b| {
        let a_time = a["modified"].as_u64().unwrap_or(0);
        let b_time = b["modified"].as_u64().unwrap_or(0);
        b_time.cmp(&a_time)
    });
    
    // Read the latest log file content
    let latest_log_content = if let Some(latest) = log_files.first() {
        if let Some(name) = latest["name"].as_str() {
            let log_path = log_dir.join(name);
            std::fs::read_to_string(&log_path).unwrap_or_else(|_| "Unable to read log file".to_string())
        } else {
            String::new()
        }
    } else {
        "No log files found".to_string()
    };
    
    Json(json!({
        "files": log_files,
        "latest_content": latest_log_content,
    }))
}

async fn get_status(State(_state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "running",
        "uptime": "N/A",
        "last_backup": "N/A",
    }))
}

async fn get_snapshots(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let config = state.config.read().await;
    
    // Build restic snapshots command
    let mut cmd = tokio::process::Command::new("restic");
    cmd.arg("snapshots");
    cmd.arg("--repo").arg(&config.restic.repository);
    cmd.arg("--json"); // Get JSON output for easier parsing
    
    // Handle password
    if let Some(ref password_cmd) = config.restic.password_command {
        cmd.arg("--password-command").arg(password_cmd);
    } else if let Some(ref password) = config.restic.password {
        cmd.env("RESTIC_PASSWORD", password);
    }
    
    // Set SSH command if provided
    if let Some(ref ssh_cmd) = config.restic.ssh_command {
        cmd.env("RESTIC_SSH_COMMAND", ssh_cmd);
    }
    
    drop(config); // Release the lock
    
    // Execute the command
    let output = cmd.output().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Restic snapshots error: {}", stderr);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    
    // Parse JSON output from restic
    let stdout = String::from_utf8_lossy(&output.stdout);
    let snapshots: Vec<serde_json::Value> = serde_json::from_str(&stdout)
        .map_err(|e| {
            eprintln!("Failed to parse snapshots JSON: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    Ok(Json(json!({
        "snapshots": snapshots,
        "count": snapshots.len()
    })))
}
