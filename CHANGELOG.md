# Changelog

All notable changes to this project are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [Unreleased]

### Added
- **#53** Collapsible multi-table mini-map (`TableMiniMap`) with seat counts, chip stacks, and click-to-navigate; coordinator `GET /api/tables/overview`
- **#55** Player seat HUD stats tooltip (VPIP, PFR, aggression factor, hands played) via `GET /api/stats/player/:address`
- **#60** Lightweight i18n (English + Spanish) with browser auto-detect and manual override in settings/header
- **#70** On-chain `player-rating` Soroban contract (ELO, min-hands leaderboard gate, recorder auth) and `/stats` rating leaderboard UI

---

## [0.1.0] — 2026-05-16

### Added
- `poker-table` Soroban contract: full Texas Hold'em state machine, betting rounds, pot/side-pot calculation, timeout auto-fold, and onchain settlement
- `zk-verifier` Soroban contract: UltraHonk proof verification using Soroban's native BN254 host functions (Protocol 25)
- `committee-registry` Soroban contract: MPC committee registration and slashing logic
- `game-hub` Soroban contract: mock Stellar Game Studio interface
- Noir ZK circuits: `deal_valid`, `reveal_board_valid`, `showdown_valid` — all proved inside TACEO coNoir MPC
- Shared Noir library (`circuits/lib`): card encoding, Poseidon2 commitments, Merkle tree, shuffle verification
- `stellar-zk-cards` reusable Rust crate: card encoding and hand evaluation for Soroban apps
- Coordinator service (Axum): orchestrates MPC sessions, submits proofs and actions to Soroban
- MPC node service: TACEO coNoir participant implementing REP3 secret sharing
- Next.js frontend: lobby, pixel-art table, Freighter wallet integration, solo mode vs AI
- Docker Compose stack for full local development
- Deploy and setup scripts for testnet
