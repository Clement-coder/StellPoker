# Contract Storage Optimization Strategy

Guide for Soroban storage patterns used in the StellPoker contracts: choosing the right storage tier, minimising footprint, bump strategy, and managing access costs. Companion to [`soroban-budget-profiling.md`](soroban-budget-profiling.md).

---

## 1. Storage Tiers

Soroban offers three storage tiers, each with a different cost model and lifetime policy.

| Tier | Key space | Lifetime | Read cost | Write cost | Use when |
|------|-----------|----------|-----------|------------|----------|
| **Instance** | Shared with the contract instance | Tied to contract deploy; extended whenever any instance entry is bumped | Cheapest: loaded once per transaction for all instance keys | Cheapest | Flags, counters, config that every call reads |
| **Persistent** | Per key | Unlimited in principle; must be actively bumped or it expires | Per-key read | Per-key write | Large per-table state that should survive indefinitely |
| **Temporary** | Per key | Short TTL, cannot be extended | Per-key read | Per-key write | Nonces, rate-limit windows, short-lived proofs |

### Rules of thumb

- Use **instance** for data that every entry point reads: `admin`, `paused`, `next_table_id`.
- Use **persistent** for per-table `TableState`, verification keys, committee epochs.
- Avoid **temporary** storage in the current contracts; nothing here is naturally short-lived.

---

## 2. How Access Costs Work

Every storage read or write charges against the transaction's *read/write byte quotas*:

| Operation | Cost driver |
|-----------|-------------|
| `storage().instance().get()` | One byte charge for the entire instance entry blob, paid once regardless of how many keys you read from instance storage in the same transaction. |
| `storage().persistent().get(&key)` | One byte charge sized to the serialised `(key, value)` pair, paid per distinct key per transaction. |
| `storage().persistent().set(&key, &val)` | Write-byte charge sized to serialised `(key, value)`. Charged every call. |
| `storage().persistent().extend_ttl(&key, …)` | No byte charge; charges CPU instructions only. |

### Measuring actual costs

```bash
stellar contract invoke \
  --id "$POKER_TABLE_CONTRACT" \
  --source ci-account \
  --rpc-url "$SOROBAN_RPC" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --simulate \
  -- get_table --table_id 0 2>&1 | grep -E "Read|Write"
```

---

## 3. Data Layout Optimisation

### 3.1 Combine frequently co-read fields

If two values are always read together, put them in the same storage entry. The current `TableState` struct bundles all per-table data into a single persistent key (`DataKey::Table(table_id)`). This means one read fetches everything: blinds, players, pot, phase. No separate reads for individual fields.

```rust
// Good: one persistent read for the whole table
let table: TableState = env.storage().persistent().get(&DataKey::Table(table_id))?;

// Avoid: three separate reads
let phase: GamePhase = env.storage().persistent().get(&DataKey::Phase(table_id))?;
let pot: i128     = env.storage().persistent().get(&DataKey::Pot(table_id))?;
let players: ...  = env.storage().persistent().get(&DataKey::Players(table_id))?;
```

The trade-off: a monolithic struct means every write re-serialises the entire blob even if only one field changed. For `TableState` (~200–600 bytes depending on player count), this is acceptable.

### 3.2 Keep `Vec` short

`soroban_sdk::Vec` serialises its elements inline. A `Vec<PlayerState>` with 6 players costs ~6× the size of one `PlayerState`. Keeping `max_players = 6` caps this.

For `side_pots` (a `Vec<SidePot>`), the maximum depth is bounded by player count, so no special treatment is needed.

### 3.3 Admin and pause flag in instance storage

The `Paused` and `Admin` keys live in instance storage, which is loaded as a unit on the first instance access. Subsequent reads in the same transaction hit the in-memory cache.

```rust
// lib.rs — correctly uses instance storage for hot paths
fn require_not_paused(env: &Env, table_id: u32) -> Result<(), PokerTableError> {
    if env.storage().instance()
        .get::<DataKey, bool>(&DataKey::Paused(table_id))
        .unwrap_or(false)
    {
        return Err(PokerTableError::ContractPaused);
    }
    Ok(())
}
```

If the pause check were in persistent storage, it would cost one extra persistent read on every entry point call.

---

## 4. Footprint Management

Soroban transactions declare a *footprint* — the set of ledger keys they will read or write — before execution. The footprint is computed automatically by `simulateTransaction`, but understanding it helps avoid surprises.

### 4.1 What goes in the footprint

| Action | Footprint entries added |
|--------|------------------------|
| `load_table(table_id)` | `DataKey::Table(table_id)` (read) |
| `save_table(table)` | `DataKey::Table(table_id)` (read + write) |
| `token::transfer(from, to, amount)` | Token balance entries for `from` and `to` |
| `env.storage().instance().get(key)` | The contract instance entry (shared for all instance keys) |

### 4.2 Avoiding unnecessary footprint entries

Avoid branching reads where one branch reads a key the other doesn't. The footprint is declared up front; if a key appears in the footprint but isn't read, you still pay its read-byte cost.

```rust
// Avoid: may or may not read DataKey::SomeOptional depending on runtime state
if condition {
    let x = env.storage().persistent().get(&DataKey::SomeOptional)?;
}

// Better: read unconditionally with `try_get` and handle `None` cheaply
let x: Option<T> = env.storage().persistent().try_get(&DataKey::SomeOptional);
```

### 4.3 Token transfer footprint

Each `token::transfer` call adds two balance entries (sender and receiver) to the footprint. When settling a multi-way pot in `pot.rs`, all player transfers are batched in a single call to `settle_hand`, keeping the footprint predictable.

---

## 5. Bump / TTL Strategies

Persistent storage entries expire after their TTL (time-to-live) passes. The `TableState` entry must survive until at least the hand is settled and all players have claimed their winnings.

### 5.1 Current strategy (read-triggered bump)

Every call to `load_table` extends the TTL:

```rust
fn load_table(env: &Env, table_id: u32) -> Result<TableState, PokerTableError> {
    let key = DataKey::Table(table_id);
    let table = env.storage().persistent().get(&key)
        .ok_or(PokerTableError::TableNotFound)?;
    env.storage().persistent().extend_ttl(
        &key,
        TABLE_TTL_THRESHOLD,   // 17 280 ledgers ≈ 1 day
        TABLE_TTL_EXTEND,      // 518 400 ledgers ≈ 30 days
    );
    Ok(table)
}
```

This means: as long as any player touches the contract within 30 days, the table never expires. An idle table that no one touches for 30 days will expire, releasing its ledger space.

### 5.2 Choosing `threshold` and `extend` values

| Value | Meaning | Guideline |
|-------|---------|-----------|
| `threshold` | If remaining TTL < threshold, extend. | Set to ~1 day so every active game extends on each call without paying the extend cost when TTL is still healthy. |
| `extend` | New TTL after extension. | Set to the longest plausible session lifetime. 30 days covers any realistic game. |

If `threshold` is set too low (e.g., 1 ledger), every read pays the extend cost. If `threshold` is too high (e.g., equal to `extend`), you extend on every call unnecessarily.

### 5.3 Verification key bump

The `zk-verifier` contract stores the UltraHonk verification key in persistent storage. It should be bumped to the maximum ledger TTL on deploy and never needs bumping during normal operation (VKs don't change):

```bash
stellar contract extend \
  --id "$ZK_VERIFIER_CONTRACT" \
  --source deployer \
  --rpc-url "$SOROBAN_RPC" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --key-xdr "$(stellar xdr encode DataKey::VerificationKey ...)" \
  --ledgers-to-extend 3110400  # ~6 months
```

### 5.4 Instance storage TTL

Instance storage TTL is bumped automatically whenever any instance key is written. You can also extend it explicitly:

```rust
env.storage().instance().extend_ttl(TABLE_TTL_THRESHOLD, TABLE_TTL_EXTEND);
```

Call this in `load_table` or `save_table` to keep the instance alive alongside the table data.

---

## 6. Persistent vs Instance: Decision Guide

```
Is the data read on nearly every call?
  ├─ YES → instance storage
  │        (one read cost for all instance keys per transaction)
  └─ NO  → Is the data large or table-specific?
             ├─ YES → persistent storage with per-table key
             └─ NO  → Consider instance storage anyway to batch the read
```

### Current allocation in StellPoker

| Data | Storage tier | Reason |
|------|-------------|--------|
| `Paused(table_id)` | Instance | Read on every guarded entry point |
| `Admin(table_id)` | Instance | Read on every admin-only function |
| `NextTableId` | Instance | Read on `create_table` only — small, cheap |
| `Table(table_id)` | Persistent | Large struct; table-specific; must outlive the session |
| `RakeBalance(table_id)` | Persistent | Separate from `TableState` to allow cheap rake reads |
| Verification key (VK) | Persistent (in zk-verifier) | Large (~KB); rarely changes |
| Committee epoch | Persistent (in committee-registry) | Epoch-scoped; managed separately |

---

## 7. Storage Access Costs in Numbers

Approximate costs as of Protocol 26 (testnet). Verify with `simulateTransaction` for your target network.

| Operation | CPU insns (approx) | Read bytes | Write bytes |
|-----------|--------------------|------------|-------------|
| `instance().get()` | ~200 | sizeof(instance blob) once | — |
| `persistent().get(&key)` | ~1 000–5 000 | sizeof(key) + sizeof(value) | — |
| `persistent().set(&key, &val)` | ~2 000–8 000 | — | sizeof(key) + sizeof(value) |
| `persistent().extend_ttl()` | ~500 | — | — |
| Token `transfer` | ~10 000–20 000 | 2× balance entry | 2× balance entry |

The dominant cost for `poker-table` is not storage but token transfers (during `join_table`, `leave_table`, and pot settlement).

---

## 8. Anti-Patterns to Avoid

| Anti-pattern | Problem | Fix |
|---|---|---|
| Per-field persistent keys | N reads for N fields | Bundle related fields into one struct |
| Reading storage inside a loop | O(n) reads for n players | Read `TableState` once, iterate in memory |
| Storing large blobs in instance | Instance entry grows with every key; all keys are loaded together | Keep large per-table data in persistent |
| Never bumping persistent entries | Entry expires; funds become inaccessible | Bump on every `load_table` call |
| Using `temporary` for game state | Short TTL; state vanishes mid-hand if ledger advances quickly | Always use `persistent` for `TableState` |

---

## 9. Auditing Storage Usage

The `scripts/audit_storage_access.py` script scans contract source for all storage calls and reports them:

```bash
python3 scripts/audit_storage_access.py
```

This script is also run in CI (see `.github/workflows/ci.yml`, `contracts` job). It catches regressions such as:
- A new persistent read inside a hot loop
- An instance key that grew to hold per-player data
- A missing `extend_ttl` after a new `persistent().set()` call

---

## 10. Further Reading

- [`soroban-budget-profiling.md`](soroban-budget-profiling.md) — How to measure CPU and memory costs with `simulateTransaction`
- [`docs/contract-callgraph.md`](contract-callgraph.md) — Which functions call which storage keys
- [Stellar Developers: Soroban Storage](https://developers.stellar.org/docs/build/smart-contracts/fundamentals/storing-data) — Official reference
- [Protocol 25 Release Notes](https://stellar.org/blog/developers/protocol-25-upgrade-guide) — BN254 host functions and storage cost changes
