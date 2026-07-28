# Database Backup and Restore Documentation

This document describes the automated backup and restore system for the StellPoker coordinator database.

## Overview

The backup system provides:
- **Database Schema & ER Diagram**: Reference available at [`docs/database-schema.md`](docs/database-schema.md)
- **Automated daily backups** with encryption support
- **Tiered retention policy**: 7 days daily, 4 weeks weekly, 12 months monthly
- **Monthly restore tests** to verify backups can be recovered
- **Integrity verification** with SHA256 checksums
- **Encrypted storage** with GPG encryption
- **Comprehensive logging** for audit trails

## Quick Start

### Setup Automated Backups

```bash
# Run the setup script
./scripts/setup-backup-cron.sh
```

This will:
1. Create backup directory structure
2. Generate configuration files
3. Set up PostgreSQL authentication
4. Configure cron jobs for automatic backups

### Manual Backup

```bash
# Create a backup now
./scripts/backup-coordinator-db.sh
```

### Restore from Backup

```bash
# List available backups
./scripts/restore-coordinator-db.sh --list

# Restore from a specific backup
./scripts/restore-coordinator-db.sh --file backups/daily/coordinator_db_daily_20240115_020000.sql

# Test restore without applying changes
./scripts/restore-coordinator-db.sh --file backups/daily/coordinator_db_daily_20240115_020000.sql --dry-run
```

### Test Backup Restoration

```bash
# Run monthly backup restore test
./scripts/test-backup-restore.sh
```

## Backup System Details

### Directory Structure

```
project-root/
├── backups/
│   ├── daily/          # Daily backups (7 days retention)
│   ├── weekly/         # Weekly backups (4 weeks retention)
│   └── monthly/        # Monthly backups (12 months retention)
├── logs/
│   ├── backup.log      # Daily backup logs
│   └── backup_test.log # Restore test logs
└── .env.backup         # Backup configuration
```

### Backup Files

Each backup includes:
- **Backup file**: `.sql` (or `.sql.gpg` if encrypted)
- **Checksum**: `.sql.sha256` for integrity verification
- **Metadata**: `.sql.metadata` with backup details

Example:
```
coordinator_db_daily_20240115_020000.sql
coordinator_db_daily_20240115_020000.sql.sha256
coordinator_db_daily_20240115_020000.sql.metadata
```

### Retention Policy

| Type    | Frequency | Retention | Schedule |
|---------|-----------|-----------|----------|
| Daily   | Every day | 7 days    | Auto     |
| Weekly  | Sundays   | 4 weeks   | Auto     |
| Monthly | 1st of month | 12 months | Auto     |

Backups are automatically promoted:
- New daily backups are created daily
- On Sundays, a weekly backup is created
- On the 1st of each month, a monthly backup is created

Old backups are automatically deleted per retention policy.

## Encryption

### Enable GPG Encryption

Set your GPG recipient ID:

```bash
export GPG_RECIPIENT="your-gpg-id@example.com"
./scripts/backup-coordinator-db.sh
```

Or configure in `.env.backup`:
```bash
GPG_RECIPIENT=your-gpg-id@example.com
```

### Verify GPG Setup

```bash
# List your GPG keys
gpg --list-keys

# Export your GPG public key
gpg --export your-gpg-id@example.com
```

### Restore Encrypted Backups

The restore script automatically handles encrypted backups:

```bash
./scripts/restore-coordinator-db.sh --file backups/monthly/coordinator_db_monthly_20240101_020000.sql.gpg
# Will prompt for GPG passphrase
```

## Authentication

### PostgreSQL Credentials

The backup scripts need database credentials. Three options:

#### 1. Using .pgpass (Recommended)

Create `~/.pgpass`:
```
localhost:5432:*:coordinator:your_password
```

Set permissions:
```bash
chmod 600 ~/.pgpass
```

#### 2. Environment Variables

```bash
export DB_HOST=localhost
export DB_PORT=5432
export DB_USER=coordinator
export DB_PASSWORD=your_password
./scripts/backup-coordinator-db.sh
```

#### 3. Configuration File

Edit `.env.backup`:
```bash
DB_HOST=localhost
DB_PORT=5432
DB_USER=coordinator
DB_PASSWORD=your_password
```

## Cron Jobs

### View Current Backups

```bash
crontab -l
```

### Edit Cron Schedule

```bash
crontab -e
```

### Default Schedule

```
# Coordinator database backup (daily at 2 AM UTC)
0 2 * * * /path/to/scripts/backup-coordinator-db.sh

# Monthly restore test (1st at 3 AM UTC)
0 3 1 * * /path/to/scripts/test-backup-restore.sh
```

### Custom Schedules

Change backup time in `crontab -e`:

```bash
# Hourly backups
0 * * * * /path/to/scripts/backup-coordinator-db.sh

# Every 4 hours
0 */4 * * * /path/to/scripts/backup-coordinator-db.sh

# Twice daily (2 AM and 2 PM UTC)
0 2,14 * * * /path/to/scripts/backup-coordinator-db.sh
```

## Restore Procedures

### Full Database Restore

1. **List available backups**:
   ```bash
   ./scripts/restore-coordinator-db.sh --list
   ```

2. **Test the restore** (without applying):
   ```bash
   ./scripts/restore-coordinator-db.sh --file backups/daily/coordinator_db_daily_20240115_020000.sql --dry-run
   ```

3. **Apply the restore**:
   ```bash
   ./scripts/restore-coordinator-db.sh --file backups/daily/coordinator_db_daily_20240115_020000.sql
   ```

The script will:
- Verify backup integrity
- Terminate existing connections
- Drop the old database
- Create a new database
- Restore from backup
- Verify restoration success

### Restore to Different Host

```bash
export DB_HOST=backup-server.example.com
export DB_PORT=5433
./scripts/restore-coordinator-db.sh --file backups/daily/coordinator_db_daily_20240115_020000.sql
```

### Restore to Different Database Name

```bash
export DB_NAME=coordinator_db_restored
./scripts/restore-coordinator-db.sh --file backups/daily/coordinator_db_daily_20240115_020000.sql
```

## Monthly Restore Test

The system includes automatic monthly tests to verify backups can be restored.

### How It Works

1. Finds the latest backup file
2. Creates an isolated test database
3. Restores the backup into the test database
4. Validates schema and data
5. Cleans up the test database
6. Generates a test report

### Manual Test

```bash
./scripts/test-backup-restore.sh
```

### Test Report

Reports are saved to:
```
backup_restore_test_YYYYMMDD_HHMMSS.log
```

Example output:
```
[2024-01-15 03:00:00] ✓ Backup file exists
[2024-01-15 03:00:05] ✓ Checksum verification passed
[2024-01-15 03:00:10] ✓ Database restored with 42 tables
[2024-01-15 03:00:15] ✓ Sanity checks passed
```

## Monitoring and Alerts

### Check Backup Status

```bash
# View recent backups
ls -lh backups/daily/ | tail -10

# Check backup logs
tail -50 logs/backup.log

# Verify backup integrity
sha256sum -c backups/daily/*.sha256
```

### Backup Metrics

```bash
# Count backups by type
echo "Daily: $(ls -1 backups/daily/ | grep -v metadata | grep -v sha256 | wc -l)"
echo "Weekly: $(ls -1 backups/weekly/ | grep -v metadata | grep -v sha256 | wc -l)"
echo "Monthly: $(ls -1 backups/monthly/ | grep -v metadata | grep -v sha256 | wc -l)"

# Total backup size
du -sh backups/
```

### Log Files

Monitor backup and restore operations:

```bash
# Real-time backup log monitoring
tail -f logs/backup.log

# Restore test logs
tail -f logs/backup_test.log

# Check for errors
grep "ERROR" logs/backup.log
```

## Troubleshooting

### Connection Issues

**Error**: "Database at localhost:5432 is not available"

Solution:
```bash
# Verify PostgreSQL is running
pg_isready -h localhost -p 5432

# Check PostgreSQL logs
psql -c "SELECT version();"

# Verify credentials
pgpass_test="localhost:5432:*:coordinator:password"
```

### Encryption Issues

**Error**: "Failed to decrypt backup file"

Solution:
```bash
# Verify GPG setup
gpg --list-keys

# Test decryption manually
gpg --decrypt backups/monthly/coordinator_db_monthly_20240101_020000.sql.gpg

# Ensure passphrase is available (use gpg-agent)
gpg-agent --daemon
```

### Space Issues

**Error**: "No space left on device"

Solution:
```bash
# Check disk space
df -h

# Check backup directory size
du -sh backups/

# Manually delete old backups (be careful!)
find backups/daily -mtime +7 -delete
```

### Restore Issues

**Error**: "Checksum verification failed"

Solution:
```bash
# Re-download or verify backup source
ls -lh backups/daily/coordinator_db_daily_*.sql*

# Manually verify (if not encrypted)
md5sum backups/daily/coordinator_db_daily_*.sql
```

## Best Practices

1. **Test Restores Regularly** - Monthly tests are automated, run manual tests quarterly
2. **Verify Encryption** - Ensure GPG keys are backed up separately
3. **Monitor Logs** - Review backup logs weekly for errors
4. **Monitor Space** - Ensure backup directory has sufficient free space
5. **Document Changes** - Note database schema changes for troubleshooting
6. **Store Offsite** - Consider replicating backups to remote storage
7. **Version Control** - Use git for database migration scripts

## Advanced Configuration

### Custom Backup Schedule

Edit crontab for different retention:
```bash
# Keep more frequent backups
0 */4 * * * /path/to/scripts/backup-coordinator-db.sh
```

### Remote Backup Storage

After backup completes:
```bash
# rsync to remote
rsync -av backups/ backup-server:/data/coordinator_backups/

# or S3
aws s3 sync backups/ s3://my-bucket/coordinator-db/
```

### Backup Compression

Add compression to backup script:
```bash
# In backup-coordinator-db.sh, modify pg_dump line:
pg_dump ... | gzip > backup.sql.gz
```

## References

- [PostgreSQL pg_dump Documentation](https://www.postgresql.org/docs/current/app-pgdump.html)
- [PostgreSQL pg_restore Documentation](https://www.postgresql.org/docs/current/app-pgrestore.html)
- [GnuPG Documentation](https://www.gnu.org/software/gpg/documentation/)
- [Cron Job Scheduling](https://en.wikipedia.org/wiki/Cron)
