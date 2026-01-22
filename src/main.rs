use anyhow::Result;
use log::{info, error, debug};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    backup: BackupConfig,
    logging: LoggingConfig,
    restic: ResticConfig,
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

#[derive(Debug, Deserialize, Serialize)]
struct ResticConfig {
    repository: String,
    #[serde(default)]
    ssh_command: Option<String>,
    #[serde(default)]
    password_command: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

fn main() -> Result<()> {
    // Check for command-line flags
    let args: Vec<String> = std::env::args().collect();
    let dry_run = args.iter().any(|arg| arg == "--dry-run" || arg == "-n");
    let verbose = args.iter().any(|arg| arg == "--verbose" || arg == "-v");

    // Read config from YAML file
    let config_path = "config.yaml";
    debug!("Reading config from: {}", config_path);
    let config_content = std::fs::read_to_string(config_path)?;
    let config: Config = serde_yaml::from_str(&config_content)?;
    debug!("Config loaded successfully");

    // Set up rolling logs with verbose level if requested
    let log_level = if verbose { "debug" } else { "info" };
    setup_logging(&config.logging, log_level)?;

    info!("Better Restic Client starting up");
    info!("Backup frequency: {}", config.backup.frequency);
    info!("Backup time: {}", config.backup.time);
    info!("Backup directories: {:?}", config.backup.directories);
    info!("Exclude directories: {:?}", config.backup.exclude);
    info!("Log directory: {:?}", config.logging.directory);
    info!("Max log size: {}", config.logging.max_size);
    info!("Dry run mode: {}", dry_run);
    info!("Verbose mode: {}", verbose);
    info!("Restic repository: {}", config.restic.repository);
    
    if let Some(ref ssh_cmd) = config.restic.ssh_command {
        debug!("SSH command configured: {}", ssh_cmd);
    }
    if let Some(ref pwd_cmd) = config.restic.password_command {
        debug!("Password command configured: {}", pwd_cmd);
    }
    if config.restic.password.is_some() {
        debug!("Direct password configured (from config.yaml)");
    }

    // Execute restic backup
    execute_restic_backup(&config.backup, &config.restic, dry_run, verbose)?;

    Ok(())
}

fn setup_logging(logging_config: &LoggingConfig, log_level: &str) -> Result<()> {
    use flexi_logger::{FileSpec, Logger, Criterion, Naming, Cleanup};

    // Expand tilde in directory path
    let log_dir = if logging_config.directory.to_string_lossy().starts_with("~") {
        let home = std::env::var("HOME")
            .map_err(|_| anyhow::anyhow!("HOME environment variable not set"))?;
        let path_str = logging_config.directory.to_string_lossy().replace("~", &home);
        PathBuf::from(path_str)
    } else {
        logging_config.directory.clone()
    };

    // Ensure log directory exists
    debug!("Creating log directory: {:?}", log_dir);
    std::fs::create_dir_all(&log_dir).map_err(|e| {
        anyhow::anyhow!(
            "Failed to create log directory {:?}: {}. \
            Please ensure you have write permissions or use a user-writable directory (e.g., ~/.local/log/restic)",
            log_dir,
            e
        )
    })?;

    // Parse max size (simple parser for MB/KB)
    let max_size_bytes = parse_size(&logging_config.max_size)?;
    debug!("Log max size: {} bytes", max_size_bytes);

    // Configure flexi_logger with size-based rotation
    Logger::try_with_env_or_str(log_level)?
        .log_to_file(FileSpec::default().directory(&log_dir).basename("restic_backup"))
        .rotate(
            Criterion::Size(max_size_bytes),
            Naming::Numbers,
            Cleanup::KeepLogFiles(3), // Keep 3 backup files
        )
        .format(flexi_logger::detailed_format)
        .start()?;

    debug!("Logging initialized with level: {}", log_level);
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

fn execute_restic_backup(backup_config: &BackupConfig, restic_config: &ResticConfig, dry_run: bool, verbose: bool) -> Result<()> {
    debug!("Building restic backup command");
    
    // Build restic backup command
    let mut cmd = Command::new("restic");
    cmd.arg("backup");

    // Add repository using --repo flag
    debug!("Setting repository: {}", restic_config.repository);
    cmd.arg("--repo").arg(&restic_config.repository);

    // Handle password: password_command takes precedence over direct password
    if let Some(ref password_cmd) = restic_config.password_command {
        debug!("Using password command for authentication");
        cmd.arg("--password-command").arg(password_cmd);
    } else if let Some(ref password) = restic_config.password {
        debug!("Using direct password from config (RESTIC_PASSWORD environment variable)");
        cmd.env("RESTIC_PASSWORD", password);
    } else {
        debug!("No password configured - restic will prompt or use default");
    }

    // Set SSH command as environment variable if provided (restic doesn't have a direct flag for this)
    if let Some(ref ssh_cmd) = restic_config.ssh_command {
        debug!("Setting SSH command environment variable: {}", ssh_cmd);
        cmd.env("RESTIC_SSH_COMMAND", ssh_cmd);
    }

    // Add verbose flag if enabled
    if verbose {
        debug!("Adding verbose flag to restic command");
        cmd.arg("--verbose");
    }

    // Add directories to backup
    debug!("Adding {} directories to backup", backup_config.directories.len());
    for dir in &backup_config.directories {
        debug!("  - Adding directory: {:?}", dir);
        cmd.arg(dir);
    }

    // Add exclude patterns
    debug!("Adding {} exclude patterns", backup_config.exclude.len());
    for exclude_path in &backup_config.exclude {
        debug!("  - Excluding: {:?}", exclude_path);
        cmd.arg("--exclude").arg(exclude_path);
    }

    // Add dry-run flag if enabled
    if dry_run {
        debug!("Adding dry-run flag");
        cmd.arg("--dry-run");
    }

    // Print the command that would be executed
    let cmd_string = format!("{:?}", cmd);
    info!("Restic command: {}", cmd_string);
    debug!("Full command details: {:?}", cmd);
    
    // In verbose mode, show a more readable command format
    if verbose {
        let mut readable_cmd = format!("restic backup --repo {}", restic_config.repository);
        if let Some(ref pwd_cmd) = restic_config.password_command {
            readable_cmd.push_str(&format!(" --password-command '{}'", pwd_cmd));
        }
        if verbose {
            readable_cmd.push_str(" --verbose");
        }
        for dir in &backup_config.directories {
            readable_cmd.push_str(&format!(" {:?}", dir));
        }
        for exclude_path in &backup_config.exclude {
            readable_cmd.push_str(&format!(" --exclude {:?}", exclude_path));
        }
        if dry_run {
            readable_cmd.push_str(" --dry-run");
        }
        debug!("Readable command: {}", readable_cmd);
    }

    if dry_run {
        info!("DRY RUN MODE: Executing restic backup with --dry-run flag");
        debug!("Executing command and capturing output");
        debug!("Command: {:?}", cmd);
        
        // Execute the command
        let output = cmd.output().map_err(|e| {
            let error_msg = format!(
                "Failed to execute restic command: {}. \
                Make sure 'restic' is installed and available in your PATH. \
                Command attempted: {:?}",
                e, cmd
            );
            eprintln!("\n=== COMMAND EXECUTION ERROR ===");
            eprintln!("{}", error_msg);
            eprintln!("===============================\n");
            anyhow::anyhow!(error_msg)
        })?;
        
        debug!("Command exit status: {:?}", output.status.code());
        debug!("Stdout length: {} bytes", output.stdout.len());
        debug!("Stderr length: {} bytes", output.stderr.len());
        
        if output.status.success() {
            info!("Dry run completed successfully");
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.is_empty() {
                info!("Output:\n{}", stdout);
            }
            
            // In verbose mode, also show stderr even if successful (might contain warnings)
            if verbose {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    debug!("Stderr output:\n{}", stderr);
                }
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let exit_code = output.status.code();
            
            error!("Dry run failed with exit code: {:?}", exit_code);
            
            // Always print detailed error information to stderr for visibility
            eprintln!("\n=== RESTIC DRY RUN FAILED ===");
            eprintln!("Exit code: {:?}", exit_code);
            eprintln!("\nCommand executed: {}", cmd_string);
            
            // Check if this is a repository initialization error
            let stderr_lower = stderr.to_lowercase();
            let is_repo_error = stderr_lower.contains("unable to open config file") 
                || stderr_lower.contains("is there a repository")
                || stderr_lower.contains("repository not found");
            
            if !stderr.is_empty() {
                eprintln!("\nStderr output:");
                eprintln!("{}", stderr);
                error!("Stderr: {}", stderr);
            }
            
            if !stdout.is_empty() {
                eprintln!("\nStdout output:");
                eprintln!("{}", stdout);
                error!("Stdout: {}", stdout);
            }
            
            // Provide helpful suggestion for repository initialization
            if is_repo_error {
                eprintln!("\n💡 SUGGESTION:");
                eprintln!("   The repository at '{}' does not exist or is not accessible.", restic_config.repository);
                eprintln!("   Initialize it first with:");
                eprintln!("   restic init --repo {}", restic_config.repository);
                if let Some(ref pwd_cmd) = restic_config.password_command {
                    eprintln!("   (with RESTIC_PASSWORD_COMMAND='{}')", pwd_cmd);
                } else if restic_config.password.is_some() {
                    eprintln!("   (with RESTIC_PASSWORD set from config)");
                }
                if let Some(ref ssh_cmd) = restic_config.ssh_command {
                    eprintln!("   (with RESTIC_SSH_COMMAND='{}')", ssh_cmd);
                }
            }
            
            // Check for password-related errors
            if stderr_lower.contains("empty password") || stderr_lower.contains("password") {
                eprintln!("\n💡 PASSWORD ERROR:");
                eprintln!("   Restic requires a password. Make sure you have configured either:");
                eprintln!("   - 'password' field in config.yaml (direct password)");
                eprintln!("   - 'password_command' field in config.yaml (command to retrieve password)");
                eprintln!("   - Or set RESTIC_PASSWORD environment variable");
            }
            
            eprintln!("=============================\n");
            
            // Create a detailed error message
            let error_msg = if !stderr.is_empty() {
                format!("Restic dry run failed (exit code: {:?}): {}", exit_code, stderr.trim())
            } else if !stdout.is_empty() {
                format!("Restic dry run failed (exit code: {:?}): {}", exit_code, stdout.trim())
            } else {
                format!("Restic dry run failed with exit code: {:?}", exit_code)
            };
            
            return Err(anyhow::anyhow!(error_msg));
        }
    } else {
        // Execute the actual backup (not dry-run)
        info!("EXECUTING BACKUP: Running restic backup");
        println!("Executing: {}", cmd_string);
        
        // Execute the command and stream output
        let output = cmd.output().map_err(|e| {
            let error_msg = format!(
                "Failed to execute restic command: {}. \
                Make sure 'restic' is installed and available in your PATH. \
                Command attempted: {:?}",
                e, cmd
            );
            eprintln!("\n=== COMMAND EXECUTION ERROR ===");
            eprintln!("{}", error_msg);
            eprintln!("===============================\n");
            anyhow::anyhow!(error_msg)
        })?;
        
        debug!("Command exit status: {:?}", output.status.code());
        debug!("Stdout length: {} bytes", output.stdout.len());
        debug!("Stderr length: {} bytes", output.stderr.len());
        
        if output.status.success() {
            info!("Backup completed successfully");
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.is_empty() {
                println!("\n{}", stdout);
                info!("Output:\n{}", stdout);
            }
            
            // In verbose mode, also show stderr even if successful (might contain warnings)
            if verbose {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    eprintln!("\n{}", stderr);
                    debug!("Stderr output:\n{}", stderr);
                }
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let exit_code = output.status.code();
            
            error!("Backup failed with exit code: {:?}", exit_code);
            
            // Always print detailed error information to stderr for visibility
            eprintln!("\n=== RESTIC BACKUP FAILED ===");
            eprintln!("Exit code: {:?}", exit_code);
            eprintln!("\nCommand executed: {}", cmd_string);
            
            // Check if this is a repository initialization error
            let stderr_lower = stderr.to_lowercase();
            let is_repo_error = stderr_lower.contains("unable to open config file") 
                || stderr_lower.contains("is there a repository")
                || stderr_lower.contains("repository not found");
            
            if !stderr.is_empty() {
                eprintln!("\nStderr output:");
                eprintln!("{}", stderr);
                error!("Stderr: {}", stderr);
            }
            
            if !stdout.is_empty() {
                eprintln!("\nStdout output:");
                eprintln!("{}", stdout);
                error!("Stdout: {}", stdout);
            }
            
            // Provide helpful suggestion for repository initialization
            if is_repo_error {
                eprintln!("\n💡 SUGGESTION:");
                eprintln!("   The repository at '{}' does not exist or is not accessible.", restic_config.repository);
                eprintln!("   Initialize it first with:");
                eprintln!("   restic init --repo {}", restic_config.repository);
                if let Some(ref pwd_cmd) = restic_config.password_command {
                    eprintln!("   (with RESTIC_PASSWORD_COMMAND='{}')", pwd_cmd);
                } else if restic_config.password.is_some() {
                    eprintln!("   (with RESTIC_PASSWORD set from config)");
                }
                if let Some(ref ssh_cmd) = restic_config.ssh_command {
                    eprintln!("   (with RESTIC_SSH_COMMAND='{}')", ssh_cmd);
                }
            }
            
            // Check for password-related errors
            if stderr_lower.contains("empty password") || stderr_lower.contains("password") {
                eprintln!("\n💡 PASSWORD ERROR:");
                eprintln!("   Restic requires a password. Make sure you have configured either:");
                eprintln!("   - 'password' field in config.yaml (direct password)");
                eprintln!("   - 'password_command' field in config.yaml (command to retrieve password)");
                eprintln!("   - Or set RESTIC_PASSWORD environment variable");
            }
            
            eprintln!("=============================\n");
            
            // Create a detailed error message
            let error_msg = if !stderr.is_empty() {
                format!("Restic backup failed (exit code: {:?}): {}", exit_code, stderr.trim())
            } else if !stdout.is_empty() {
                format!("Restic backup failed (exit code: {:?}): {}", exit_code, stdout.trim())
            } else {
                format!("Restic backup failed with exit code: {:?}", exit_code)
            };
            
            return Err(anyhow::anyhow!(error_msg));
        }
    }

    Ok(())
}
