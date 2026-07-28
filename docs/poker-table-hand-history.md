# Contract-Level Hand History

The `poker-table` contract keeps the last **16** completed hands per table in a
circular on-chain buffer. Each settled hand is archived with the players who
were dealt in, the final board, a summary of the betting, and how the pot was
split — enough for a client to reconstruct and replay the hand without having
scraped the event stream while it was happening.

This complements (rather than replaces) the browser-side history in
`app/src/lib/hand-history.ts`: the client store is richer and unbounded but
local to one browser, while the contract buffer is authoritative and readable
by anyone.

## Storage layout

| Key | Tier | Contents |
|-----|------|----------|
| `HandRecord(table_id, slot)` | Persistent | One archived hand |
| `HandHistoryMeta(table_id)` | Persistent | `next_slot`, `stored`, `total_archived` |

Each record lives under its own key so archiving a hand writes exactly one
record plus the meta entry — the cost of settling does not grow with how much
history a table has accumulated. Records share the table's TTL policy
(~1 day threshold, ~30 day extension) and are bumped when read.

`next_slot` advances modulo the capacity, so hand 17 overwrites hand 1. Hands
that have aged out of the window are still recoverable from the
`hand_archived` event stream, which is emitted on every archive.

## Record contents

```rust
pub struct HandRecord {
    pub hand_number: u32,
    pub players: Vec<Address>,      // seat-ordered
    pub board: Vec<u32>,            // community cards at the end of the hand
    pub actions: Vec<ActionRecord>, // betting summary, in order
    pub payouts: Vec<Payout>,       // one entry per paid seat
    pub total_pot: i128,            // before rake
    pub rake: i128,
    pub showdown: bool,             // false when the hand ended on a fold
    pub settled_ledger: u32,
}
```

`ActionRecord` carries the seat, the betting round it happened in, the action
kind, and the chips that action put into the pot (`0` for fold and check).
Payouts resolve the winning seat to an address at archive time, so a reader
never has to cross-reference the live table — whose seating may have changed
since the hand was played.

`board` may hold fewer than five cards: a hand that ends on a fold is archived
with whatever community cards had been revealed at that point.

### Bounds

A hand's action summary is capped at **64** entries
(`history::MAX_ACTIONS_PER_HAND`). Beyond that the summary is truncated rather
than the record refused, which keeps the settlement write bounded regardless of
how long a hand runs. In practice a six-handed hand takes well under 64
actions across all four betting rounds.

## Reading history

| Function | Returns |
|----------|---------|
| `get_hand_history(table_id, limit)` | Up to `limit` records, **newest first**. `limit = 0` reads the whole window. |
| `get_hand(table_id, hand_number)` | One record, or `None` once it has been evicted. |
| `get_hand_history_meta(table_id)` | How many records are retained and how many hands have been archived in total. |
| `hand_history_capacity()` | The buffer size (16). |

All four are read-only and require no authorization.

```bash
# Latest three hands at table 0
stellar contract invoke --id "$POKER_TABLE" -- get_hand_history --table_id 0 --limit 3

# One specific hand
stellar contract invoke --id "$POKER_TABLE" -- get_hand --table_id 0 --hand_number 12
```

Pass a small `limit` when you only need the most recent hands — reading the
full window loads all 16 records into the transaction footprint.

## Events

Every archive emits:

```
topics: ("hand_archived", table_id)
data:   (hand_number, slot, total_pot)
```

Indexers that want history beyond the 16-hand window should follow this event
and fetch the record before it is overwritten.
