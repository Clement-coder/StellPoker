# Soroban Transaction Budget Profiling

Guide for measuring and optimizing the CPU instruction and memory budget consumed by Stellar Poker's Soroban contracts — in particular the `zk-verifier` contract, which performs UltraHonk proof verification using the BN254 host functions introduced in Protocol 25 and 26.

---

## 1. Background: Soroban Resource Budgets

Every Soroban transaction runs against a **resource budget**: a set of hard limits on CPU instructions, memory bytes, ledger entry reads/writes, and transaction size. Exceeding any limit results in `ResourceLimitExceeded` and the transaction is rejected.

The relevant limits for ZK proof verification:

| Resource | Typical limit (testnet, as of Protocol 26) |
|---|---|
| CPU instructions | 100,000,000 (100M) |
| Memory (bytes) | 40,000,000 (40 MB) |
| Read bytes | 200,000 |
| Write bytes | 40,000 |

UltraHonk verification for the `showdown_valid` circuit (237 018 backend gates) is the most expensive operation in the system. Profiling this path is essential before mainnet deployment.

---

## 2. Using `simulateTransaction` for Budget Profiling

The Soroban RPC exposes a `simulateTransaction` method that executes a transaction off-chain and returns detailed resource consumption without broadcasting to the network.

### 2.1 Via Stellar CLI (`stellar contract invoke --simulate`)

The `stellar` CLI wraps `simulateTransaction` automatically when invoked without `--send`. The output includes a resource breakdown:

```bash
stellar contract invoke \
  --id "$ZK_VERIFIER_CONTRACT" \
  --source test-account \
  --rpc-url http://localhost:8000/soroban/rpc \
  --network-passphrase "Test SDF Network ; September 2015" \
  --simulate \
  -- verify_proof \
  --circuit_type '"ShowdownValid"' \
  --proof "$(cat /tmp/showdown_proof.hex)" \
  --public_inputs "$(cat /tmp/showdown_inputs.hex)"
```

The `--simulate` flag prints the estimated resource footprint:

```
Simulated transaction:
  CPU instructions: 45,231,847
  Memory bytes:     12,450,012
  Read bytes:       8,640
  Write bytes:      256
  Ledger entries read:  3
  Ledger entries write: 1
  Min fee:          412 stroops
```

### 2.2 Via Raw JSON-RPC

For automation or CI integration, call `simulateTransaction` directly:

```bash
# Build and XDR-encode the transaction first (stellar CLI can do this
# with --build-only), then:
curl -s -X POST http://localhost:8000/soroban/rpc \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "simulateTransaction",
    "params": {
      "transaction": "<base64-xdr-transaction>"
    }
  }' | jq .result.cost
```

Response:

```json
{
  "cpuInsns": "45231847",
  "memBytes": "12450012"
}
```

Parse both fields; only `cpuInsns` and `memBytes` are returned by `simulateTransaction`. Read/write byte counts require looking at `simulateTransaction.transactionData.resources`.

### 2.3 Extracting Full Resource Details

```bash
curl -s -X POST http://localhost:8000/soroban/rpc \
  -H 'Content-Type: application/json' \
  -d '{ "jsonrpc":"2.0","id":1,"method":"simulateTransaction","params":{"transaction":"<xdr>"}}' \
  | jq '{
      cpu:        .result.cost.cpuInsns,
      mem:        .result.cost.memBytes,
      read_bytes: .result.transactionData | @base64d | fromjson | .resources.readBytes,
      write_bytes:.result.transactionData | @base64d | fromjson | .resources.writeBytes
    }'
```

> Note: `transactionData` is base64-encoded XDR. Use the Stellar SDK to decode it programmatically in scripts rather than `jq`-chaining base64 decoding.

---

## 3. Interpreting Budget Logs

### What the numbers mean

| Field | Meaning |
|-------|---------|
| `cpuInsns` | Abstract CPU "fuel" units consumed. Each host function call has a fixed metered cost; BN254 operations are heavily metered. |
| `memBytes` | Peak working-set bytes allocated during execution. Includes Wasm linear memory and host objects (BN254 points, byte arrays). |
| `readBytes` | Total bytes read from the ledger (contract WASM + storage entries). |
| `writeBytes` | Total bytes written to the ledger (proof records, state updates). |

### Reading the logs from the coordinator

When the coordinator submits a proof and receives a `ResourceLimitExceeded` error, it logs:

```
WARN stellar invoke hit ResourceLimitExceeded; retrying with higher instruction leeway (attempt 1/4)
```

This tells you that the **default** resource envelope was not enough and the CLI is retrying with `--instruction-leeway`. If all four retry levels fail, the contract call is rejected.

The `INSTRUCTION_LEEWAY_STEPS` in `services/coordinator/src/soroban/mod.rs` are: `[0, 50_000_000, 200_000_000, 500_000_000]`. The leeway is added on top of the simulated cost, not the hard limit. If the `--instruction-leeway 500_000_000` attempt still fails, the contract function genuinely exceeds the network's hard ceiling.

### Baseline measurements

Run `simulateTransaction` for each circuit type on testnet and record the results in a table. Update after any circuit or contract change:

| Operation | Circuit | CPU insns | Memory (MB) |
|-----------|---------|-----------|-------------|
| `commit_deal` | `deal_valid` | ~18M | ~6 MB |
| `reveal_board` | `reveal_board_valid` | ~20M | ~7 MB |
| `settle_showdown` | `showdown_valid` | ~45M | ~12 MB |

> These are illustrative values. Run `simulateTransaction` on your target network to get accurate numbers; host function costs are version-specific and have changed between Protocol 25 and 26.

---

## 4. Optimizing Contract Functions

### 4.1 Reduce ledger reads

Each `get`/`try_get` call on Stellar storage counts against `readBytes`. Batch reads where possible:

```rust
// Expensive: two separate storage reads
let vk = env.storage().persistent().get::<_, Bytes>(&DataKey::VerificationKey(circuit_type));
let state = env.storage().persistent().get::<_, ContractState>(&DataKey::State);

// Better: if both are needed, check whether the struct can be combined
// into a single storage entry to halve the read count
```

### 4.2 Avoid redundant proof re-serialization

The proof bytes (16 256 bytes for UltraHonk) arrive as a `Bytes` argument and must be deserialized into `Vec<u8>` for verification. Avoid copying the byte array multiple times:

```rust
// Avoid: converts to Vec, then slices, then re-allocates
let proof_vec: Vec<u8> = proof.to_vec();
let proof_slice = &proof_vec[..PROOF_BYTES as usize];
let result = verifier.verify(proof_slice, ...);

// Better: pass the Soroban Bytes reference directly if the verifier
// can accept a reference to the host-managed byte buffer
```

### 4.3 Cache the verification key

The VK is stored in persistent ledger storage and is read on every `verify_proof` call. This is a large read (a few KB). The Soroban runtime caches ledger reads within a single transaction, so if the VK is read once at the start of `verify_proof`, subsequent accesses within the same call are free. There is no cross-transaction caching — each transaction pays the read cost once.

### 4.4 Use `instance` storage for frequently-read admin state

The paused/unpaused flag and the admin address are read on every call. Put them in `instance` storage rather than `persistent` storage:

```rust
// Less efficient: persistent storage (per-key read cost)
env.storage().persistent().get::<_, bool>(&DataKey::Paused)

// More efficient: instance storage (read once per contract instance per tx)
env.storage().instance().get::<_, bool>(&DataKey::Paused)
```

### 4.5 Minimize public input parsing

The `zk-verifier` contract parses public inputs from a raw `Bytes` argument by slicing 32-byte field elements. Ensure slice bounds are checked once at the start (not per-element):

```rust
// Check total size once
if public_inputs.len() != DEAL_BYTES {
    return Err(VerifierError::PublicInputSizeError);
}
// Then slice freely — no per-element bounds check needed
let deck_root = public_inputs.slice(0..32);
```

---

## 5. Setting Gas Limits (Fee and Resource Bounds)

Soroban transactions include a **resource footprint** (declared upfront) and a **fee**. The fee is computed from the declared footprint.

### Automatic footprint via `simulateTransaction`

The Stellar CLI's `contract invoke` automatically calls `simulateTransaction` first and uses the simulated footprint to set the transaction resources. You do not need to set resource bounds manually unless you are building transactions programmatically.

For programmatic transaction building (e.g., in the coordinator's Rust code if it is ever updated to use the SDK instead of shelling out):

```rust
use stellar_sdk::{TransactionBuilder, SorobanResources};

// Get footprint from simulateTransaction
let sim_response = rpc.simulate_transaction(&tx).await?;
let resources = SorobanResources::from_simulation(sim_response);

// Add a 20% safety buffer to CPU instructions
let cpu_with_buffer = resources.cpu_insns * 12 / 10;
let resources_with_buffer = resources.with_cpu_insns(cpu_with_buffer);

let tx = builder
    .add_soroban_resources(resources_with_buffer)
    .build();
```

### Instruction leeway

The `--instruction-leeway N` CLI flag adds N extra CPU instructions to the declared footprint *beyond* what `simulateTransaction` measured. Use this when the actual on-chain execution might consume more than the simulation (e.g., due to non-deterministic read ordering). For deterministic contracts like `zk-verifier`, a 10–20% buffer is usually sufficient.

---

## 6. CI Budget Regression Guard

Add a simulation check to CI to catch budget regressions before they reach mainnet. The following shell snippet can be run in a GitHub Actions step after deploying to testnet:

```bash
#!/usr/bin/env bash
set -euo pipefail

MAX_CPU=60000000  # 60M instructions — conservative headroom below 100M limit

simulate_cost() {
    local circuit=$1
    local proof_file=$2
    local inputs_file=$3

    stellar contract invoke \
      --id "$ZK_VERIFIER_CONTRACT" \
      --source ci-account \
      --rpc-url "$SOROBAN_RPC" \
      --network-passphrase "$NETWORK_PASSPHRASE" \
      --simulate \
      -- verify_proof \
      --circuit_type "\"${circuit}\"" \
      --proof "$(cat "$proof_file")" \
      --public_inputs "$(cat "$inputs_file")" \
      2>&1 | grep "CPU instructions" | awk '{print $3}' | tr -d ','
}

for circuit in DealValid RevealBoardValid ShowdownValid; do
    cpu=$(simulate_cost "$circuit" "ci/proofs/${circuit}_proof.hex" "ci/proofs/${circuit}_inputs.hex")
    echo "${circuit}: ${cpu} CPU instructions"
    if [[ "$cpu" -gt "$MAX_CPU" ]]; then
        echo "FAIL: ${circuit} exceeds CPU budget (${cpu} > ${MAX_CPU})"
        exit 1
    fi
done

echo "All circuits within CPU budget."
```

---

## 7. Quick Profiling Workflow

```
1. Make a change to a circuit or contract.
2. Recompile: ./scripts/compile-circuits.sh && cargo build -p zk-verifier
3. Redeploy to local: ./scripts/deploy-local.sh
4. Simulate each circuit type:
     stellar contract invoke ... --simulate -- verify_proof ...
5. Record CPU/memory in the baseline table (section 3).
6. Compare against previous baseline.
7. If CPU > 80M insns for any circuit, investigate with the
   optimization techniques in this guide and in docs/circuit-optimization-cookbook.md.
```
