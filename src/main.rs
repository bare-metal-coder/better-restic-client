use anyhow::Result;
use log::info;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    backup: BackupConfig,
    logging: LoggingConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackupConfig {
    frequency: String,
    time: String,
    directories: Vec<PathBuf>,
    exclude: Vec<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize)]
struct LoggingConfig {
    directory: PathBuf,
    max_size: String, // e.g., "10MB", "100KB"
}

fn main() -> Result<()> {
    // Read config from YAML file
    let config_path = "config.yaml";
    let config_content = std::fs::read_to_string(config_path)?;
    let config: Config = serde_yaml::from_str(&config_content)?;

    // Set up rolling logs
    setup_logging(&config.logging)?;

    info!("Better Restic Client starting up");
    info!("Backup frequency: {}", config.backup.frequency);
    info!("Backup time: {}", config.backup.time);
    info!("Backup directories: {:?}", config.backup.directories);
    info!("Exclude directories: {:?}", config.backup.exclude);
    info!("Log directory: {:?}", config.logging.directory);
    info!("Max log size: {}", config.logging.max_size);

    // TODO: Implement restic backup logic here

    Ok(())
}

fn setup_logging(logging_config: &LoggingConfig) -> Result<()> {
    use flexi_logger::{FileSpec, Logger, Criterion, Naming, Cleanup};

    // Ensure log directory exists
    std::fs::create_dir_all(&logging_config.directory)?;

    // Parse max size (simple parser for MB/KB)
    let max_size_bytes = parse_size(&logging_config.max_size)?;

    // Configure flexi_logger with size-based rotation
    Logger::try_with_env_or_str("info")?
        .log_to_file(FileSpec::default().directory(&logging_config.directory).basename("restic_backup"))
        .rotate(
            Criterion::Size(max_size_bytes),
            Naming::Numbers,
            Cleanup::KeepLogFiles(3), // Keep 3 backup files
        )
        .format(flexi_logger::detailed_format)
        .start()?;

    Ok(())
}

fn parse_size(size_str: &str) -> Result<u64> {
    let size_str = size_str.trim().to_uppercase();
    let (number, unit) = if size_str.ends_with("MB") {
        (
            size_str[..size_str.len() - 2].trim(),
            "MB",
        )
    } else if size_str.ends_with("KB") {
        (
            size_str[..size_str.len() - 2].trim(),
            "KB",
        )
    } else if size_str.ends_with("GB") {
        (
            size_str[..size_str.len() - 2].trim(),
            "GB",
        )
    } else {
        return Err(anyhow::anyhow!("Invalid size format. Use MB, KB, or GB"));
    };

    let number: u64 = number.parse()?;
    let bytes = match unit {
        "KB" => number * 1024,
        "MB" => number * 1024 * 1024,
        "GB" => number * 1024 * 1024 * 1024,
        _ => return Err(anyhow::anyhow!("Invalid unit")),
    };

    Ok(bytes)
}

