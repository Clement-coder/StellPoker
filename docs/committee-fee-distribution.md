# Committee Fee Distribution

Rake collected by a poker table is paid into the committee registry and split
among the **active** MPC nodes in proportion to their stake. The nodes with the
most collateral at risk from a slash earn the most for doing the work.

Deposit, distribution and withdrawal are three separate steps:

```
poker-table                committee-registry
───────────                ──────────────────
withdraw_rake()  ──chips──▶ deposit_rake(from, amount)   → FeePool
                            distribute_fees()             → PendingReward(node) per node
                            withdraw_rewards(node)  ──chips──▶ node wallet
```

Splitting them means a table can pay in cheaply, the proportional split runs
once per batch rather than once per deposit, and a node chooses when to take
its balance out.

The two contracts stay decoupled — the registry has no reference to any table,
and the table has no reference to the registry. A table admin withdraws rake in
the usual way and then deposits it here.

## Deposit

```rust
deposit_rake(from: Address, amount: i128)
```

`from.require_auth()` is enforced and the chips move in the same call. Anyone
may deposit; there is nothing to gain from paying into a pool you do not
control. `amount` must be positive.

The token is the registry's `stake_token`, configured at `initialize`.

## Distribution

```rust
distribute_fees() -> i128   // returns the amount credited
```

Permissionless. It only moves the pool into per-node ledgers by a fixed rule,
so there is nothing to gain from calling it — or from withholding it.

Each active node is credited:

```
share = floor(pool * stake / total_stake)
```

The floors leave a few stroops of dust. That dust **stays in the pool** and
rolls into the next distribution rather than being stranded or burned.

Two cases are deliberate no-ops that leave the pool intact:

- **No active nodes** (or total stake is zero) — the pool waits for the next
  epoch instead of being burned.
- **Empty pool** — returns 0 without touching storage.

Only nodes with `active == true` are paid. A node that has been deregistered or
slashed out of the active set stops earning, but **keeps whatever it has
already been credited** for work it performed.

## Withdrawal

```rust
withdraw_rewards(member: Address) -> i128
```

`member.require_auth()` is enforced. The accrued balance must have reached
`min_withdrawal`, which stops a node paying more in transaction fees than a
dust payout is worth. Withdrawal is all-or-nothing — the balance is zeroed and
transferred in full.

```rust
set_min_withdrawal(admin: Address, min_withdrawal: i128)  // admin only
```

The threshold defaults to `0` (no minimum). Operators running on a network with
meaningful transaction fees should set one that comfortably exceeds the cost of
a withdrawal transaction.

## Accounting

The registry's token balance is:

```
sum(member stakes) + fee_pool.undistributed + fee_pool.pending
```

Fees and stake are tracked separately, so a fee withdrawal can never draw down
another node's staked collateral — asserted directly in
`fees_never_draw_down_staked_collateral`.

Read the whole picture in one call:

```rust
get_fee_pool() -> FeePoolState {
    undistributed: i128,      // deposited, not yet split
    pending: i128,            // credited to nodes, not yet withdrawn
    total_distributed: i128,  // lifetime total credited
    min_withdrawal: i128,     // current threshold
}

get_pending_reward(member: Address) -> i128
```

## Pausing

`deposit_rake`, `distribute_fees` and `withdraw_rewards` all revert while the
registry is paused. The reads stay available.

## Events

```
topics: ("rake_deposited",)        data: (from, amount, new_pool_total)
topics: ("fees_distributed",)      data: (distributed, node_count, dust_carried_over)
topics: ("rewards_withdrawn",)     data: (member, amount)
topics: ("min_withdrawal_updated",) data: min_withdrawal
```
