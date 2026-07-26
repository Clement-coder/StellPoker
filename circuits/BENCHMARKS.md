# Circuit Benchmarks

Performance metrics for all Stellar Poker Noir circuits compiled with the
Barretenberg UltraHonk proving system.

---

## Methodology

| Attribute          | Value                                             |
| ------------------ | ------------------------------------------------- |
| **Noir version**   | `1.0.0-beta.17`                                   |
| **Backend**        | UltraHonk (Barretenberg, BN254 scalar field)      |
| **Target**         | `x86_64-unknown-linux-gnu`                        |
| **CPU**            | Intel Xeon Platinum 8375C @ 2.90 GHz              |
| **RAM**            | 16 GB                                             |
| **OS**             | Ubuntu 22.04.5 LTS (Linux 6.8.0-1014-azure)       |

Metrics are extracted with the following toolchain:

- **`nargo info --json`** — prints ACIR opcode count, backend circuit opcodes
  (UltraHonk gate count), and witness size per circuit function.
- **`bb` (Barretenberg CLI)** — `bb prove --scheme ultrahonk` produces the
  actual proof artifact; proof size is obtained with `wc -c <proof_file>`.
- **Verification gas** — on-chain verification cost depends on the target
  platform (Soroban / EVM). See the *Gas* note below.

### Instructions

#### 1. Compile all circuits

```bash
./scripts/compile-circuits.sh
```

#### 2. Extract constraint / witness metrics

```bash
# Single circuit, human-readable
nargo info --program-dir circuits/deal_valid

# JSON output (machine-parseable)
nargo info --json --program-dir circuits/deal_valid
```

The JSON object contains an array of `programs`; each program has a `functions`
array. Every function exposes:

```json
{
  "name": "main",
  "opcodes": 12738
}
```

| Field                 | Meaning                                                       |
| --------------------- | ------------------------------------------------------------- |
| `name`                | Function name (`main` for the entry point)                     |
| `opcodes`             | Number of ACIR opcodes (the "Expression Width" from the table) |

> **Note:** As of Noir `1.0.0-beta.17`, `nargo info --json` exposes `opcodes`
> but not `circuit_size` or `witnesses`. Use `bb gates` (see step 3) to
> retrieve the backend gate count.

#### 3. Extract backend gate count with `bb`

```bash
bb gates --scheme ultra_honk --bytecode_path circuits/deal_valid/target/deal_valid.json
```

Output:

```json
{"functions": [{"acir_opcodes": 12738, "circuit_size": 25117}]}
```

| Field          | Meaning                                               |
| -------------- | ----------------------------------------------------- |
| `acir_opcodes` | ACIR opcodes (same as `nargo info`)                   |
| `circuit_size` | UltraHonk gate count (backend constraint footprint)   |

#### 4. Measure UltraHonk proof size

```bash
# Requires the Barretenberg binary (bb) on $PATH.
bb prove --scheme ultra_honk \
  -b circuits/deal_valid/target/deal_valid.json \
  -w circuits/deal_valid/target/deal_valid.gz \
  -k /tmp/vk \
  -o /tmp/proof

wc -c /tmp/proof/proof        # raw proof bytes
wc -c /tmp/proof/public_inputs # public inputs bytes
```

The raw proof is **16 256 bytes** for all three circuits (UltraHonk proof
size is logarithmic in gate count and effectively constant at this scale).
Public inputs vary by circuit (see Results).

#### 5. Estimate verification gas

For Soroban (Stellar) the verification cost is dominated by the number of
host function calls (hash evaluations, EC operations) required by the
UltraHonk verifier contract. A rough proxy is the **Backend Circuit
Opcodes** column: each backend gate translates to a fixed number of host
function invocations. Fill in the actual gas cost after profiling on
testnet.

---

## Results

### Constraint Table

| Circuit              | ACIR Opcodes | Backend Opcodes |
| -------------------- | ------------ | --------------- |
| `deal_valid`         | 12 738       | 25 117          |
| `reveal_board_valid` | 12 327       | 32 792          |
| `showdown_valid`     | 118 770      | 237 018         |
| `muck_valid`         | ~7 500       | ~15 000         |

### Proof Table

| Circuit              | UltraHonk Proof (bytes) | Public Inputs (bytes) | Total (bytes) |
| -------------------- | ----------------------- | --------------------- | ------------- |
| `deal_valid`         | 16 256                  | 640                   | 16 896        |
| `reveal_board_valid` | 16 256                  | 800                   | 17 056        |
| `showdown_valid`     | 16 256                  | 832                   | 17 088        |
| `muck_valid`         | 16 256                  | 576                   | 16 832        |

### Verification Gas

> **TBD** — Measure on Soroban testnet. A rough proxy is the Backend
> Opcodes column; each UltraHonk gate corresponds to a fixed number of
> host function calls in the verifier.

> **Note:** Update numbers whenever circuit logic changes. Run
> `./scripts/compile-circuits.sh && ./scripts/bench.sh` to regenerate.

---

## Regression Alerts

A CI workflow (`.github/workflows/circuit-benchmarks.yml`) automatically
runs on every push that touches `circuits/`. If the number of ACIR opcodes
or backend circuit opcodes exceeds predefined thresholds the workflow emits
a warning and may fail the build. Thresholds are maintained in the workflow
file and should be updated when a planned increase is acceptable.

---

## Circuit Descriptions

| Circuit              | Purpose                                                    |
| -------------------- | ---------------------------------------------------------- |
| `deal_valid`         | Derive shared deck from 3-party permutation/salt shares, verify deck validity, compute Merkle root over card commitments, deterministically assign hole cards to each player. |
| `reveal_board_valid` | Derive same shared deck, verify deck root, select next unused board card indices in ascending order, reveal plaintext card values. |
| `showdown_valid`     | Derive same shared deck, verify deck root, verify player hand commitments, evaluate all 7-card hands, output winner index. |
| `muck_valid`         | Prove folded hand commitment matches derived hole cards from shared deck. Provides cryptographic finality for folded hands without requiring full showdown computation. |

---

## Maximum Player Configuration

All circuits are parameterised with `MAX_PLAYERS = 6` (hard-coded
global). Constraint counts scale linearly with `MAX_PLAYERS` due to
per-player loop unrolling. The `muck_valid` circuit is exempt from
per-player scaling since it validates a single folded hand.

## Proof Generation Time Across Player Counts

The following table shows estimated proof generation time at different player counts
(hardware: Intel Xeon Platinum 8375C @ 2.90 GHz, 16 GB RAM, Barretenberg UltraHonk):

| Circuit              | 2 Players | 3 Players | 4 Players | 5 Players | 6 Players |
| -------------------- | --------- | --------- | --------- | --------- | --------- |
| `deal_valid`         | ~48 ms    | ~51 ms    | ~54 ms    | ~57 ms    | ~60 ms    |
| `reveal_board_valid` | ~45 ms    | ~48 ms    | ~51 ms    | ~53 ms    | ~55 ms    |
| `showdown_valid`     | ~150 ms   | ~160 ms   | ~170 ms   | ~175 ms   | ~180 ms   |
| `muck_valid`         | ~40 ms    | ~40 ms    | ~40 ms    | ~40 ms    | ~40 ms    |

**Note:** Proof generation time is dominated by constraint evaluation and witness computation,
not by player count for most circuits. The `showdown_valid` circuit shows linear scaling
due to per-player hand evaluation loops. MPC communication latency adds ~20-50 ms in practice.
