# Production Configuration Reference

Configuration knobs for the coordinator service, infrastructure, and load balancer,
grouped by concern. All coordinator settings are environment variables unless noted.

---

## Connection Pool

The coordinator uses a `sqlx` PostgreSQL connection pool (configured in `services/coordinator/src/db.rs`).

| Knob | Where | Default | Notes |
|---|---|---|---|
| `DATABASE_URL` | env | — (in-memory) | Full Postgres URL. Omit to run stateless. |
| `max_connections` | hardcoded in `db.rs` | `10` | Maximum pool connections. Raise for high-concurrency tables; keep below RDS `max_connections` (default ~100 on `db.t3.medium`). |
| `acquire_timeout` | hardcoded in `db.rs` | `5 s` | Time before a pool acquire fails. Lower values surface saturation quickly; raise if you see spurious timeouts under burst load. |
| `db_instance_class` | Terraform variable | `db.t3.medium` | RDS instance type. Scale up to `db.r6g.large` for production workloads. |
| `db_allocated_storage` | Terraform variable | `100 GB` | Initial storage; RDS autoscales by default. |
| `rds_multi_az` | Terraform variable | `true` | Enables standby replica for failover. |

> To change the hardcoded pool limits, edit `PgPoolOptions::new().max_connections(...)` in `services/coordinator/src/db.rs` and redeploy.

---

## Cache TTLs

### Stats cache (in-memory, coordinator)

The `/api/stats` endpoint builds a response from the Horizon event index and caches it.

| Knob | Where | Default | Notes |
|---|---|---|---|
| `STATS_CACHE_TTL_SECS` | env | `30` | How long a built stats response is reused before being rebuilt. Increase to reduce Horizon polling under load. |
| `STATS_POLL_SECS` | env | `30` | Horizon polling interval for the background indexer. Should be ≤ `STATS_CACHE_TTL_SECS`. |

### Session archive TTLs (coordinator)

Completed/timed-out MPC sessions are archived in `.tmp/session-archives` before being evicted from memory.

| Knob | Env variable | Default | Notes |
|---|---|---|---|
| Archive TTL | `SESSION_ARCHIVE_TTL_SECS` | `3600` (1 h) | How long a non-running session stays in memory before being written to the archive store. |
| Purge TTL | `SESSION_PURGE_TTL_SECS` | `86400` (24 h) | How long an archived session file is kept on disk before deletion. |
| Archive path | `SESSION_ARCHIVE_PATH` | `.tmp/session-archives` | Filesystem path for archive JSON files. Mount a persistent volume in k8s. |

### CloudFront CDN TTLs (Terraform / `aws_cdn.tf`)

| Path / behavior | Variable | Default | Notes |
|---|---|---|---|
| Default (all API paths) | `cdn_default_ttl` | `3600 s` | **Set to `0`** for `/api/*` to avoid caching dynamic API responses. |
| Default max | `cdn_max_ttl` | `86400 s` | Upper bound on `Cache-Control: max-age` from origin. |
| `/api/health` | hardcoded | default=`60 s`, max=`300 s` | Short TTL; health checks should not be stale for long. |
| `/static/*` | hardcoded | default=`86400 s`, max=`31536000 s` | Immutable static assets — intentionally long. |

A CloudFront invalidation is automatically triggered for `/api/*` on `terraform apply`.

---

## Worker Thread Count

The coordinator is an async Tokio service; it does not expose a thread-count flag. Thread pool sizing comes from the Tokio runtime and from ECS/Fargate CPU allocation.

| Knob | Where | Default | Notes |
|---|---|---|---|
| `coordinator_cpu` | Terraform variable | `512` (0.5 vCPU) | Fargate CPU units. Tokio's multi-thread runtime uses all available vCPUs. Set to `1024`+ in production. |
| `coordinator_memory` | Terraform variable | `1024 MB` | Fargate task memory. ZK proof coordination is CPU-bound; memory rarely bottlenecks. |
| `coordinator_desired_count` | Terraform variable | `2` | ECS service replicas behind the ALB. Scale horizontally for throughput. |
| `mpc_node_cpu` | Terraform variable | `1024` (1 vCPU) | MPC nodes are the compute-heavy component; increase for faster proof generation. |
| `mpc_node_memory` | Terraform variable | `2048 MB` | MPC node memory. Circuits load ~500 MB of CRS data per node. |
| HPA (k8s) | `infrastructure/helm/coordinator/values.yaml` → `autoscaling` | disabled | `minReplicas: 1`, `maxReplicas: 3`, CPU target `70%`, memory target `80%`. Enable with `autoscaling.enabled: true`. |

For the Tokio runtime itself, set `TOKIO_WORKER_THREADS` to pin the thread count explicitly (Tokio defaults to the logical CPU count).

---

## Database Query Optimization

Relevant RDS parameter group settings (`infrastructure/terraform/aws_rds.tf`):

| Parameter | Production value | Staging value | Notes |
|---|---|---|---|
| `log_statement` | `ddl` | `all` | Logs only schema changes in prod to avoid log spam. |
| `log_duration` | `0` (off) | `1` (on) | Disable per-query duration logging in prod. |
| `log_min_duration_statement` | `1000 ms` | `0 ms` | Logs queries that take longer than 1 s in prod. Use to surface slow queries. Tune down to `500` if you need finer-grained visibility. |

Additional recommendations:
- Enable **Performance Insights** (auto-enabled when `enable_monitoring = true`, 7-day retention).
- RDS Enhanced Monitoring is set to a 60-second interval when monitoring is enabled.
- The coordinator runs `sqlx::migrate!` on every startup — migrations are idempotent.

---

## Load Balancer Settings

AWS ALB is configured in `infrastructure/terraform/aws_ecs.tf` and `variables.tf`.

| Knob | Variable / hardcode | Default | Notes |
|---|---|---|---|
| Enable ALB | `enable_alb` | `true` | Disable only for local/staging single-instance deploys. |
| Health check path | `alb_health_check_path` | `/api/health` | Coordinator readiness endpoint. |
| Health check interval | `alb_health_check_interval` | `30 s` | How often the ALB probes each target. Lower to `10 s` for faster failover. |
| Health check timeout | `alb_health_check_timeout` | `5 s` | Must be < interval. Increase only if the coordinator is slow to start. |
| HTTP/2 | hardcoded | enabled | `enable_http2 = true` on the ALB. |
| Cross-zone LB | hardcoded | enabled | Distributes traffic evenly across AZs. |
| Deletion protection | derived | `true` in prod | Prevents accidental ALB removal in production. |

Kubernetes / Helm (coordinator chart, `infrastructure/helm/coordinator/values.yaml`):

| Knob | Default | Notes |
|---|---|---|
| `livenessProbe.initialDelaySeconds` | `15` | Increase if the coordinator is slow to load the CRS on startup. |
| `livenessProbe.periodSeconds` | `20` | How often k8s checks liveness. |
| `readinessProbe.periodSeconds` | `10` | How often k8s checks readiness before sending traffic. |
| `readinessProbe.failureThreshold` | `3` | Consecutive failures before pod is marked unready. |
| `resources.requests.cpu` | `200m` | HPA uses this as the baseline for CPU scaling. |
| `resources.limits.cpu` | `1` | Hard cap. Raise for proof-heavy workloads. |
| `resources.requests.memory` | `256Mi` | |
| `resources.limits.memory` | `512Mi` | |

---

## MPC Session Lifecycle

| Knob | Env variable | Default | Notes |
|---|---|---|---|
| Session timeout | `SESSION_TIMEOUT_SECS` | `300` (5 min) | MPC sessions not completed within this window are cancelled and cleaned up. Increase if proof generation is slow (e.g., large circuits on under-provisioned nodes). |
| GC sweep interval | hardcoded in `session_gc.rs` | `30 s` | How frequently the GC task scans for timed-out sessions. Not user-configurable. |

---

## Quick Reference: Key Env Variables

```bash
# Database
DATABASE_URL=postgres://coordinator:password@host:5432/coordinator

# Stats cache
STATS_CACHE_TTL_SECS=30
STATS_POLL_SECS=30

# Session lifecycle
SESSION_TIMEOUT_SECS=300
SESSION_ARCHIVE_TTL_SECS=3600
SESSION_PURGE_TTL_SECS=86400
SESSION_ARCHIVE_PATH=.tmp/session-archives

# Tokio thread count (optional override)
TOKIO_WORKER_THREADS=4

# Logging
REQUEST_LOG_FORMAT=json   # json | (default human-readable)
RUST_LOG=info             # info | debug | warn
```

For Terraform variables, create a `terraform.tfvars` file in `infrastructure/terraform/`:

```hcl
coordinator_cpu            = 1024
coordinator_memory         = 2048
coordinator_desired_count  = 2
mpc_node_cpu               = 2048
mpc_node_memory            = 4096
db_instance_class          = "db.r6g.large"
cdn_default_ttl            = 0       # disable caching for API responses
alb_health_check_interval  = 10
enable_monitoring          = true
```
