# Better Restic Client

A minimal Rust application for configuring and managing restic backups.

## Features

- YAML-based configuration
- Configurable backup frequency and time
- Multiple backup directories
- Exclude patterns
- Rolling log files with configurable max size

## Configuration

Edit `config.yaml` to configure your backups:

- `backup.frequency`: Backup frequency (e.g., "daily", "weekly")
- `backup.time`: Time to run backups (e.g., "02:00")
- `backup.directories`: List of directories to backup
- `backup.exclude`: List of directories to exclude
- `logging.directory`: Directory for log files
- `logging.max_size`: Maximum log file size (e.g., "10MB", "100KB")

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run
```

Make sure to create and edit `config.yaml` before running.

