# Multi-Table Play

A wallet may sit at any number of tables at the same time. There is no
per-player cap anywhere in the stack — the only limits are each table's
`max_players` and how much capital the player is willing to put up.

## Contract

`join_table` has always allowed a wallet to take seats at different tables; the
`AlreadySeated` check is scoped to a single table and only stops a wallet
taking two seats at the *same* one. What was missing was a way to find those
seats again.

```rust
get_player_tables(player: Address) -> Vec<u32>
get_player_table_count(player: Address) -> u32
```

The index is maintained on `join_table` and `leave_table`, so it holds only
live seats. Re-joining a table already in the list is a no-op rather than a
duplicate entry. Leaving the last table removes the entry entirely rather than
leaving an empty vector to pay rent on.

Its length is the number of seats the wallet holds, which is bounded in
practice by the buy-in each seat costs. It is a convenience index for clients —
nothing in the contract consults it to make a decision, so it can never reject
a join.

Tables are fully independent: a hand playing out at one table does not touch
another's phase, pot, or hand history. `test_tables_stay_independent_across_concurrent_seats`
pins that down.

## Transaction sequencing

This is the part that actually breaks under concurrency.

Every Stellar transaction is signed against the source account's current
sequence number, and the network accepts exactly one transaction per sequence.
Two tables firing an action in the same tick would both read the same sequence
from `getAccount` and the second would be rejected with `txBAD_SEQ`.

`app/src/lib/onchain.ts` chains submissions **per source account**: each one
reads the sequence only after the previous transaction has been sent, so the
numbers never collide.

Three details matter:

- **Per address, not global.** Different wallets never block each other.
- **Only up to send.** Waiting for confirmation happens outside the queue, so
  two tables can have transactions in flight at once and neither waits on the
  other's confirmation.
- **Failures don't poison the chain.** A rejected signature (a player dismissing
  the wallet prompt at one table) must not stall every later submission. The
  caller still sees the original rejection.

The coordinator's request nonces (`app/src/lib/api.ts`) were already safe —
`nextNonce()` is a monotonic module-level counter, so parallel calls cannot
produce a duplicate.

## Frontend navigation

`app/src/lib/open-tables.ts` tracks which tables the player has open, scoped
per wallet address in localStorage — the same pattern as `alias-store.ts`.
Where the contract index holds authoritative *seats*, this is the client's
view: it also remembers tables the player is only watching, the play mode each
was opened in, and when each was last visited.

Two entry points use it:

- **`TableTabs`** renders a strip of the player's other open tables above the
  table header, so switching seats is one click. It only appears once there is
  somewhere else to go, so a single-tabling player never sees it.
- **The lobby's join screen** lists open tables under "YOUR TABLES", so a player
  returning after a reload can drop straight back in instead of retyping IDs.

The strip caps at 12 entries and evicts the least recently visited. That caps
the *strip*, never how many tables a wallet may be seated at.

Visit stamps are forced strictly past the highest on record rather than taken
raw from `Date.now()`: millisecond resolution is coarse enough that switching
quickly between tables would otherwise stamp several identically and leave the
eviction order ambiguous.
