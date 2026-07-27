# Partial Buy-Ins and Rebuys

A seated player can top their stack up between hands for **any** amount that
leaves them inside the table's `[min_buy_in, max_buy_in]` band. A rebuy does
not have to be a full buy-in — a player ground down to 40 chips at a 100/1000
table can add anywhere from 60 (back up to the minimum) to 960 (up to the
maximum).

## Entry point

```rust
rebuy(table_id: u32, player: Address, amount: i128) -> i128  // returns the new stack
```

`player.require_auth()` is enforced, so only the wallet itself can add chips to
its seat.

### Rules

| Check | Error |
|-------|-------|
| Table is in `Waiting` or `Settlement` | `CannotRebuyDuringActiveHand` |
| Caller is seated at the table | `PlayerNotAtTable` |
| `rebuy_count < max_rebuys` (when the limit is non-zero) | `RebuyLimitReached` |
| `amount > 0` | `InvalidRebuyAmount` |
| `amount <= max_buy_in` (one rebuy is never more than a full buy-in) | `InvalidRebuyAmount` |
| `stack + amount <= max_buy_in` | `InvalidRebuyAmount` |
| `stack + amount >= min_buy_in` | `InvalidRebuyAmount` |

Rebuys are restricted to between hands. Allowing chips in mid-hand would change
what an opponent is playing against after they had already committed to a pot,
and would break the side-pot accounting, which assumes a player's effective
stack is fixed once the hand starts.

The token transfer happens before the stack is credited, so a failed transfer
leaves no phantom chips at the table.

## Configuration

`TableConfig::max_rebuys` caps how many times a player may top up during one
session. **`0` means unlimited**, which is the default and matches the previous
behaviour of the contract (where no rebuy existed at all).

```rust
set_max_rebuys(table_id: u32, max_rebuys: u32)  // admin only
```

Lowering the limit below what a player has already used simply stops them
rebuying again; it never claws chips back.

A "session" runs from `join_table` to `leave_table`. Leaving and rejoining
starts a fresh count — the player has withdrawn their stack and bought in
again, so they are subject to the same limit as any new arrival.

## Per-player accounting

`PlayerState` tracks two new fields:

- `total_buy_in` — every chip deposited this session (initial buy-in plus all
  rebuys). Session profit is `stack - total_buy_in`.
- `rebuy_count` — rebuys used, checked against `max_rebuys`.

Both are readable in one call:

```rust
get_player_buy_in(table_id: u32, player: Address) -> (i128, u32)  // (total_buy_in, rebuy_count)
```

## Chip conservation

The contract's token balance always equals `sum(stacks) + pot + rake_balance`.
A rebuy adds `amount` to both sides of that identity in the same transaction,
so the invariant holds across partial top-ups exactly as it does across
buy-ins. This is asserted directly in
`test_rebuy_preserves_chip_conservation`.

## Events

```
topics: ("player_rebuy", table_id)
data:   (player, amount, new_stack, rebuy_count)
```

```
topics: ("max_rebuys_updated", table_id)
data:   max_rebuys
```
