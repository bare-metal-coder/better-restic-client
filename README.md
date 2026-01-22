# Better Restic Client

A minimal Rust application for configuring and managing restic backups.

## Features

- YAML-based configuration
- Configurable backup frequency and time
- Multiple backup directories
- Exclude patterns
- Rolling log files with configurable max size
- Restic command execution with dry-run support
- Command preview mode
- **Modern Web UI** - Beautiful dashboard for viewing configuration, logs, and status

## Configuration

Edit `config.yaml` to configure your backups:

- `backup.frequency`: Backup frequency (e.g., "daily", "weekly")
- `backup.time`: Time to run backups (e.g., "02:00")
- `backup.directories`: List of directories to backup
- `backup.exclude`: List of directories to exclude
- `logging.directory`: Directory for log files
- `logging.max_size`: Maximum log file size (e.g., "10MB", "100KB")

## Environment Variables

Before running, make sure to export the required restic environment variables:

```bash
export RESTIC_REPOSITORY='sftp:desktop-backup:/restic-imac'
export RESTIC_SSH_COMMAND='ssh -F ~/.ssh/config'
export RESTIC_PASSWORD_COMMAND='security find-generic-password -a atlas -s restic-desktop-backup -w'
```

These can also be added to your shell profile (e.g., `~/.bashrc` or `~/.zshrc`) or exported from a separate script.

## Building

```bash
cargo build --release
```

## Running

### Print Commands (Default Mode)

By default, the client will print the restic commands that would be executed:

```bash
cargo run
```

### Dry Run Mode

To execute restic commands in dry-run mode (test without actually backing up):

```bash
cargo run -- --dry-run
# or
cargo run -- -n
```

This will execute `restic backup` with the `--dry-run` flag to show what would be backed up without actually performing the backup.

### Web UI Mode

Launch a modern web-based dashboard to view configuration, logs, and status:

```bash
cargo run -- --ui
# or
cargo run -- -u
```

This will start a web server at `http://127.0.0.1:3000` where you can:
- View and inspect your configuration
- Browse YAML configuration file
- View real-time logs with syntax highlighting
- Check system status

The web UI features a modern, responsive design with tabs for easy navigation.

## Example Output

When running without `--dry-run`, you'll see the command that would be executed:

```
Restic command: Command { program: "restic", args: ["backup", "/path/to/dir", "--exclude", "/path/to/exclude"] }
PRINT MODE: Command to execute:
Command { program: "restic", args: ["backup", "/path/to/dir", "--exclude", "/path/to/exclude"] }

To execute this command, run it manually or use --dry-run to test it.
```

When running with `--dry-run`, the command will be executed and you'll see the output from restic.

