# Integration Tests

End-to-end tests that exercise the full stack: MPC nodes → coordinator → Soroban contracts.

## Prerequisites

A running local stack:
```bash
docker-compose up -d
```

## Run

```bash
python3 scripts/test-flow.py
```

To run the real circuit-sequencing integration test for Noir/Barretenberg:

```bash
python3 -m unittest tests.test_circuit_sequence
```

Or against testnet:
```bash
NETWORK=testnet python3 scripts/test-flow.py
```

Or against the checked-in staging environment:
```bash
TEST_ENV_FILE=.env.staging \
COORDINATOR_URL=$(grep '^NEXT_PUBLIC_COORDINATOR_URL=' .env.staging | cut -d= -f2-) \
python3 scripts/test-flow.py
```

For the full staging smoke path, use:
```bash
./scripts/run-staging-checks.sh
```

## Coverage

| Test | Description |
|---|---|
| `test_solo_hand` | Full solo hand: deal → betting → showdown → settlement |
| `test_multiplayer_hand` | 2-player hand with fold |
| `test_zk_proof_verification` | Verifies deal/reveal/showdown proofs are accepted onchain |
| `test_timeout_autofold` | Player misses action window, auto-fold triggers |
| `test_committee_slashing` | Misbehaving MPC node gets slashed in committee-registry |

See `scripts/test-flow.py` for the full implementation.

## Soak / endurance test

`scripts/soak-test.py` runs a 24-hour continuous load against the coordinator
and detects memory leaks, connection leaks, and latency degradation.

### Quick start

```bash
# Install dependencies
pip install requests psutil

# 5-minute smoke run
python3 scripts/soak-test.py --duration 300

# Full 24-hour soak (requires docker-compose up)
python3 scripts/soak-test.py

# Soak against a remote coordinator, saving a JSON report
python3 scripts/soak-test.py --base-url http://my-host:8080 --report soak-report.json

# Include process monitoring and periodic restarts (pass coordinator PID)
python3 scripts/soak-test.py --pid $(pgrep coordinator) --duration 3600
```

### What it measures

| Metric | Fail threshold |
|---|---|
| RSS memory growth | > 512 MB |
| Request error rate | > 5 % |
| p99 latency | > 5 000 ms |
| Throughput slowdown (start vs end) | > 3× |

### How it works

- **`WORKER_THREADS` (default 4)** concurrent threads each run a tight loop of
  synthetic game cycles: `create-table → join → request-deal → player-action(s)
  → request-reveal/flop → request-showdown → /api/health`.
- A **snapshot** is taken every 60 s and printed to stdout.
- If `--pid` is supplied, RSS and open-file-descriptor counts are sampled via
  `psutil` and included in every snapshot.
- The coordinator is sent `SIGHUP` every hour (configurable via `--no-restart`)
  to simulate rolling restarts and verify graceful recovery.
- At the end of the run a `PASS` / `FAIL` verdict is printed and, if
  `--report` is given, a machine-readable JSON report is written.
