#![no_std]
#![allow(deprecated)]

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, Symbol, Vec};

mod constant_time;

/// Committee Registry contract.
///
/// Manages MPC committee membership, staking bonds, and slashing hooks.
/// The committee is responsible for:
/// - Shuffling the deck via MPC
/// - Generating ZK proofs via coNoir
/// - Delivering private cards to players
/// - Responding to reveal requests within timeout
#[contract]
pub struct CommitteeRegistryContract;

#[contracttype]
#[derive(Clone, Debug)]
pub struct CommitteeMember {
    pub address: Address,
    pub stake: i128,
    pub endpoint: soroban_sdk::String, // MPC node endpoint URL
    pub region: soroban_sdk::String,   // Geographic region (e.g., us-east-1)
    pub active: bool,
    pub slash_count: u32,
    /// Total stake delegated to this node by external delegators.
    pub total_delegated_stake: i128,
    /// Accumulated rewards per unit of delegated stake, scaled by REWARD_SCALE.
    /// Increases monotonically whenever fees are distributed to this node.
    pub rewards_per_stake: i128,
    /// Node-operator fee rate in basis points (0–10000). The node keeps this
    /// fraction of any fee distribution; the remainder goes to delegators.
    pub fee_rate_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct CommitteeEpoch {
    pub epoch_id: u32,
    pub members: Vec<Address>,
    pub threshold: u32, // Minimum members needed (2 of 3)
    pub start_ledger: u32,
    pub end_ledger: u32, // 0 = no end (current epoch)
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GamePhase {
    Deal,
    Reveal,
    Showdown,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct GameLiveness {
    pub game_id: u32,
    pub phase: GamePhase,
    pub last_progress_ledger: u32,
    pub affected_players: Vec<Address>,
    pub resolved: bool,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct TimeoutConfig {
    pub deal_ledgers: u32,
    pub reveal_ledgers: u32,
    pub showdown_ledgers: u32,
}

/// Per-(delegator, node) delegation record.
///
/// The `debt_snapshot` is the value of `CommitteeMember::rewards_per_stake` at
/// the moment this delegation was last checkpointed.  Pending rewards owed to
/// the delegator equal:
///
///   (rewards_per_stake_now - debt_snapshot) * amount / REWARD_SCALE
#[contracttype]
#[derive(Clone, Debug)]
pub struct DelegationRecord {
    pub delegator: Address,
    pub node: Address,
    /// Amount currently delegated (stroop).
    pub amount: i128,
    /// Accumulated rewards already credited but not yet claimed (stroop).
    pub pending_rewards: i128,
    /// Snapshot of `CommitteeMember::rewards_per_stake` at last checkpoint.
    pub debt_snapshot: i128,
}

/// An undelegation request waiting out the cooldown period.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UndelegationRequest {
    pub delegator: Address,
    pub node: Address,
    /// Amount to be returned once the cooldown expires.
    pub amount: i128,
    /// Ledger number after which the amount can be withdrawn.
    pub unlock_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub enum RegistryKey {
    Admin,
    StakeToken,
    MinStake,
    Member(Address),
    CurrentEpoch,
    Epoch(u32),
    SlashEvent(u32), // slash event counter
    TimeoutConfig,
    Game(u32),
    Paused,
    AllMembers,
    /// Per-(delegator, node) active delegation record.
    Delegation(Address, Address),
    /// Per-(delegator, node) pending undelegation awaiting cooldown.
    PendingUndelegation(Address, Address),
    /// Cooldown period in ledgers before undelegated stake can be withdrawn.
    DelegationCooldown,
    /// Index of all delegators for a given node (for slashing propagation).
    NodeDelegators(Address),
}

/// Fixed-point scale for `rewards_per_stake`. Using 1e12 gives sub-stroop
/// precision even for nodes with only a few hundred delegators.
const REWARD_SCALE: i128 = 1_000_000_000_000;

#[contractimpl]
impl CommitteeRegistryContract {
    /// Initialize the registry.
    ///
    /// * `delegation_cooldown_ledgers` — number of ledgers a delegator must
    ///   wait between calling `undelegate` and being able to `withdraw_undelegation`.
    ///   A value of 0 disables the cooldown (useful for tests).
    pub fn initialize(
        env: Env,
        admin: Address,
        stake_token: Address,
        min_stake: i128,
        delegation_cooldown_ledgers: u32,
    ) {
        admin.require_auth();
        assert!(
            !env.storage().instance().has(&RegistryKey::Admin),
            "already initialized"
        );

        env.storage().instance().set(&RegistryKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&RegistryKey::StakeToken, &stake_token);
        env.storage()
            .instance()
            .set(&RegistryKey::MinStake, &min_stake);
        env.storage().instance().set(
            &RegistryKey::TimeoutConfig,
            &TimeoutConfig {
                deal_ledgers: 120,
                reveal_ledgers: 120,
                showdown_ledgers: 120,
            },
        );
        env.storage()
            .instance()
            .set(&RegistryKey::DelegationCooldown, &delegation_cooldown_ledgers);
    }

    /// Admin configures timeout windows for the phases that depend on MPC nodes.
    pub fn set_timeout_config(
        env: Env,
        admin: Address,
        deal_ledgers: u32,
        reveal_ledgers: u32,
        showdown_ledgers: u32,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        assert!(deal_ledgers > 0, "deal timeout must be positive");
        assert!(reveal_ledgers > 0, "reveal timeout must be positive");
        assert!(showdown_ledgers > 0, "showdown timeout must be positive");

        let config = TimeoutConfig {
            deal_ledgers,
            reveal_ledgers,
            showdown_ledgers,
        };
        env.storage()
            .instance()
            .set(&RegistryKey::TimeoutConfig, &config);
        env.events().publish(
            (Symbol::new(&env, "timeout_config_updated"),),
            (deal_ledgers, reveal_ledgers, showdown_ledgers),
        );
    }

    /// Admin records the phase and affected players for a game. Poker-table or
    /// coordinator integrations call this whenever MPC-dependent progress moves.
    pub fn track_game_phase(
        env: Env,
        admin: Address,
        game_id: u32,
        phase: GamePhase,
        affected_players: Vec<Address>,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        assert!(!affected_players.is_empty(), "no affected players");

        let liveness = GameLiveness {
            game_id,
            phase: phase.clone(),
            last_progress_ledger: env.ledger().sequence(),
            affected_players: affected_players.clone(),
            resolved: false,
        };
        env.storage()
            .persistent()
            .set(&RegistryKey::Game(game_id), &liveness);
        env.events().publish(
            (Symbol::new(&env, "game_phase_tracked"), game_id),
            (phase, affected_players),
        );
    }

    /// Report an MPC node that failed to respond within the tracked phase
    /// timeout. The node is slashed immediately and the slashed amount is split
    /// among affected players; any odd stroop goes to the earliest listed player.
    pub fn report_timeout(env: Env, game_id: u32, node_id: Address) -> i128 {
        let mut game: GameLiveness = env
            .storage()
            .persistent()
            .get(&RegistryKey::Game(game_id))
            .expect("game not tracked");
        assert!(!game.resolved, "timeout already resolved");

        let config: TimeoutConfig = env
            .storage()
            .instance()
            .get(&RegistryKey::TimeoutConfig)
            .expect("not initialized");
        let timeout = Self::timeout_for_phase(&config, &game.phase);
        assert!(
            env.ledger().sequence() >= game.last_progress_ledger + timeout,
            "timeout not reached"
        );

        let slashed = Self::slash_member_stake(&env, &node_id, Symbol::new(&env, "timeout"));
        Self::redistribute_slashed_stake(&env, &game.affected_players, slashed);

        game.resolved = true;
        env.storage()
            .persistent()
            .set(&RegistryKey::Game(game_id), &game);
        env.events().publish(
            (Symbol::new(&env, "timeout_reported"), game_id),
            (node_id, game.phase, slashed),
        );
        slashed
    }

    /// Pause the registry (admin only). All mutable operations revert while paused.
    /// NOTE: for production consider a timelock or multi-sig for unpause.
    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::Admin)
            .expect("not initialized");
        assert!(constant_time::address_eq(&env, &admin, &stored_admin), "not admin");
        env.storage().instance().set(&RegistryKey::Paused, &true);
        env.events()
            .publish((Symbol::new(&env, "registry_paused"),), admin);
    }

    /// Unpause the registry (admin only).
    /// NOTE: for production consider a timelock or multi-sig here.
    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::Admin)
            .expect("not initialized");
        assert!(constant_time::address_eq(&env, &admin, &stored_admin), "not admin");
        env.storage().instance().set(&RegistryKey::Paused, &false);
        env.events()
            .publish((Symbol::new(&env, "registry_unpaused"),), admin);
    }

    /// Returns true if the registry is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<RegistryKey, bool>(&RegistryKey::Paused)
            .unwrap_or(false)
    }

    /// Register as a committee member with a stake and region metadata.
    pub fn register_member(
        env: Env,
        member: Address,
        stake: i128,
        endpoint: soroban_sdk::String,
        region: soroban_sdk::String,
        fee_rate_bps: u32,
    ) {
        member.require_auth();
        assert!(
            !env.storage()
                .instance()
                .get::<RegistryKey, bool>(&RegistryKey::Paused)
                .unwrap_or(false),
            "contract paused"
        );

        assert!(fee_rate_bps <= 10_000, "fee_rate_bps must be <= 10000");

        let min_stake: i128 = env
            .storage()
            .instance()
            .get(&RegistryKey::MinStake)
            .expect("not initialized");
        assert!(stake >= min_stake, "insufficient stake");

        // Transfer stake to contract
        let token_addr: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::StakeToken)
            .unwrap();
        let token = token::Client::new(&env, &token_addr);
        token.transfer(&member, &env.current_contract_address(), &stake);

        let member_state = CommitteeMember {
            address: member.clone(),
            stake,
            endpoint,
            region,
            active: true,
            slash_count: 0,
            total_delegated_stake: 0,
            rewards_per_stake: 0,
            fee_rate_bps,
        };

        env.storage()
            .persistent()
            .set(&RegistryKey::Member(member.clone()), &member_state);

        // Maintain list of all members for discovery
        let mut all_members: Vec<Address> = env
            .storage()
            .instance()
            .get(&RegistryKey::AllMembers)
            .unwrap_or_else(|| Vec::new(&env));

        let mut exists = false;
        for i in 0..all_members.len() {
            if all_members.get(i).unwrap() == member {
                exists = true;
                break;
            }
        }
        if !exists {
            all_members.push_back(member.clone());
            env.storage()
                .instance()
                .set(&RegistryKey::AllMembers, &all_members);
        }

        env.events()
            .publish((Symbol::new(&env, "member_registered"),), member);
    }

    /// Withdraw stake and deregister (only when not in active epoch).
    pub fn deregister_member(env: Env, member: Address) -> i128 {
        member.require_auth();
        assert!(
            !env.storage()
                .instance()
                .get::<RegistryKey, bool>(&RegistryKey::Paused)
                .unwrap_or(false),
            "contract paused"
        );

        let mut m: CommitteeMember = env
            .storage()
            .persistent()
            .get(&RegistryKey::Member(member.clone()))
            .expect("not a member");

        // Check not in active epoch
        if let Some(epoch) = Self::get_current_epoch(env.clone()) {
            for i in 0..epoch.members.len() {
                assert!(
                    constant_time::address_ne(&env, &epoch.members.get(i).unwrap(), &member),
                    "cannot deregister during active epoch"
                );
            }
        }

        let stake = m.stake;
        m.active = false;
        m.stake = 0;

        // Return stake
        let token_addr: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::StakeToken)
            .unwrap();
        let token = token::Client::new(&env, &token_addr);
        token.transfer(&env.current_contract_address(), &member, &stake);

        env.storage()
            .persistent()
            .set(&RegistryKey::Member(member.clone()), &m);

        env.events()
            .publish((Symbol::new(&env, "member_deregistered"),), member);

        stake
    }

    /// Admin removes a node from committee due to threshold failures.
    /// Emits NodeDeregistered event and marks member inactive.
    pub fn deregister_node_on_failure(env: Env, admin: Address, node: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::Admin)
            .expect("not initialized");
        assert!(constant_time::address_eq(&env, &admin, &stored_admin), "not admin");
        assert!(
            !env.storage()
                .instance()
                .get::<RegistryKey, bool>(&RegistryKey::Paused)
                .unwrap_or(false),
            "contract paused"
        );

        let mut m: CommitteeMember = env
            .storage()
            .persistent()
            .get(&RegistryKey::Member(node.clone()))
            .expect("node not registered");

        assert!(m.active, "node already inactive");

        m.active = false;
        m.slash_count = m.slash_count.saturating_add(1);

        env.storage()
            .persistent()
            .set(&RegistryKey::Member(node.clone()), &m);

        env.events().publish(
            (Symbol::new(&env, "node_deregistered"), node.clone()),
            (Symbol::new(&env, "failure_threshold"), m.slash_count),
        );
    }

    /// Admin creates a new committee epoch with selected members.
    pub fn create_epoch(env: Env, admin: Address, members: Vec<Address>, threshold: u32) -> u32 {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::Admin)
            .expect("not initialized");
        assert!(constant_time::address_eq(&env, &admin, &stored_admin), "not admin");
        assert!(
            !env.storage()
                .instance()
                .get::<RegistryKey, bool>(&RegistryKey::Paused)
                .unwrap_or(false),
            "contract paused"
        );
        assert!(
            members.len() >= threshold,
            "not enough members for threshold"
        );

        // Verify all members are registered and active
        for i in 0..members.len() {
            let addr = members.get(i).unwrap();
            let m: CommitteeMember = env
                .storage()
                .persistent()
                .get(&RegistryKey::Member(addr.clone()))
                .expect("member not registered");
            assert!(m.active, "member not active");
        }

        // Close previous epoch
        let prev_epoch_id: u32 = env
            .storage()
            .instance()
            .get(&RegistryKey::CurrentEpoch)
            .unwrap_or(0);

        if prev_epoch_id > 0 {
            let mut prev: CommitteeEpoch = env
                .storage()
                .persistent()
                .get(&RegistryKey::Epoch(prev_epoch_id))
                .unwrap();
            prev.end_ledger = env.ledger().sequence();
            env.storage()
                .persistent()
                .set(&RegistryKey::Epoch(prev_epoch_id), &prev);
        }

        let epoch_id = prev_epoch_id + 1;
        let epoch = CommitteeEpoch {
            epoch_id,
            members: members.clone(),
            threshold,
            start_ledger: env.ledger().sequence(),
            end_ledger: 0,
        };

        env.storage()
            .persistent()
            .set(&RegistryKey::Epoch(epoch_id), &epoch);
        env.storage()
            .instance()
            .set(&RegistryKey::CurrentEpoch, &epoch_id);

        env.events()
            .publish((Symbol::new(&env, "epoch_created"), epoch_id), members);

        epoch_id
    }

    /// Trigger a slashing event against a committee member.
    /// Called by PokerTable contract when committee fails to act within timeout.
    pub fn report_slash(env: Env, reporter: Address, member: Address, reason: Symbol) {
        reporter.require_auth();
        assert!(
            !env.storage()
                .instance()
                .get::<RegistryKey, bool>(&RegistryKey::Paused)
                .unwrap_or(false),
            "contract paused"
        );

        // In production, verify reporter is an authorized PokerTable contract
        // For v1, any address can report (admin will adjudicate)

        Self::slash_member_record(&env, &member, reason);
    }

    /// Return all registered members that are currently active.
    pub fn get_active_members(env: Env) -> Vec<CommitteeMember> {
        let all_addresses: Vec<Address> = env
            .storage()
            .instance()
            .get(&RegistryKey::AllMembers)
            .unwrap_or_else(|| Vec::new(&env));

        let mut active_members = Vec::new(&env);
        for i in 0..all_addresses.len() {
            let addr = all_addresses.get(i).unwrap();
            let m: CommitteeMember = env
                .storage()
                .persistent()
                .get(&RegistryKey::Member(addr))
                .expect("member state missing");
            if m.active {
                active_members.push_back(m);
            }
        }
        active_members
    }

    /// View the current epoch.
    pub fn get_current_epoch(env: Env) -> Option<CommitteeEpoch> {
        let epoch_id: u32 = env
            .storage()
            .instance()
            .get(&RegistryKey::CurrentEpoch)
            .unwrap_or(0);

        if epoch_id == 0 {
            return None;
        }

        env.storage()
            .persistent()
            .get(&RegistryKey::Epoch(epoch_id))
    }

    /// View a member's state.
    pub fn get_member(env: Env, member: Address) -> CommitteeMember {
        env.storage()
            .persistent()
            .get(&RegistryKey::Member(member))
            .expect("not a member")
    }

    pub fn get_timeout_config(env: Env) -> TimeoutConfig {
        env.storage()
            .instance()
            .get(&RegistryKey::TimeoutConfig)
            .expect("not initialized")
    }

    pub fn get_game_liveness(env: Env, game_id: u32) -> GameLiveness {
        env.storage()
            .persistent()
            .get(&RegistryKey::Game(game_id))
            .expect("game not tracked")
    }

    // -------------------------------------------------------------------------
    // Stake delegation
    // -------------------------------------------------------------------------

    /// Delegate `amount` of stake tokens to a committee node.
    ///
    /// The delegator earns a share of fees distributed to the node (minus the
    /// node's `fee_rate_bps` commission).  Multiple calls by the same delegator
    /// to the same node accumulate; the reward checkpoint is updated on each
    /// call so pending rewards are never lost.
    pub fn delegate(env: Env, delegator: Address, node: Address, amount: i128) {
        delegator.require_auth();
        assert!(
            !env.storage()
                .instance()
                .get::<RegistryKey, bool>(&RegistryKey::Paused)
                .unwrap_or(false),
            "contract paused"
        );
        assert!(amount > 0, "amount must be positive");

        let mut m: CommitteeMember = env
            .storage()
            .persistent()
            .get(&RegistryKey::Member(node.clone()))
            .expect("node not registered");
        assert!(m.active, "node not active");

        // Pull tokens from the delegator into this contract.
        let token_addr: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::StakeToken)
            .unwrap();
        let token = token::Client::new(&env, &token_addr);
        token.transfer(&delegator, &env.current_contract_address(), &amount);

        // Load or initialise the delegation record, checkpoint rewards first.
        let key = RegistryKey::Delegation(delegator.clone(), node.clone());
        let mut rec: DelegationRecord = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(DelegationRecord {
                delegator: delegator.clone(),
                node: node.clone(),
                amount: 0,
                pending_rewards: 0,
                debt_snapshot: m.rewards_per_stake,
            });

        // Settle any rewards earned on the *existing* amount before changing it.
        rec.pending_rewards += Self::calc_pending(&rec, m.rewards_per_stake);
        rec.debt_snapshot = m.rewards_per_stake;
        rec.amount += amount;

        env.storage().persistent().set(&key, &rec);

        // Update node totals.
        m.total_delegated_stake += amount;
        env.storage()
            .persistent()
            .set(&RegistryKey::Member(node.clone()), &m);

        // Maintain the node → delegator index so slashing can iterate them.
        let idx_key = RegistryKey::NodeDelegators(node.clone());
        let mut delegators: Vec<Address> = env
            .storage()
            .persistent()
            .get(&idx_key)
            .unwrap_or_else(|| Vec::new(&env));
        let mut already_indexed = false;
        for i in 0..delegators.len() {
            if delegators.get(i).unwrap() == delegator {
                already_indexed = true;
                break;
            }
        }
        if !already_indexed {
            delegators.push_back(delegator.clone());
            env.storage().persistent().set(&idx_key, &delegators);
        }

        env.events().publish(
            (Symbol::new(&env, "delegated"), node),
            (delegator, amount),
        );
    }

    /// Begin the undelegation process.  The `amount` is removed from the
    /// active delegation immediately (stopping reward accrual on it) and
    /// placed in a `PendingUndelegation` record.  Only one pending undelegation
    /// per (delegator, node) pair is allowed at a time; call
    /// `withdraw_undelegation` first if one already exists.
    pub fn undelegate(env: Env, delegator: Address, node: Address, amount: i128) {
        delegator.require_auth();
        assert!(
            !env.storage()
                .instance()
                .get::<RegistryKey, bool>(&RegistryKey::Paused)
                .unwrap_or(false),
            "contract paused"
        );
        assert!(amount > 0, "amount must be positive");

        assert!(
            !env.storage()
                .persistent()
                .has(&RegistryKey::PendingUndelegation(
                    delegator.clone(),
                    node.clone()
                )),
            "existing undelegation pending; withdraw first"
        );

        let mut m: CommitteeMember = env
            .storage()
            .persistent()
            .get(&RegistryKey::Member(node.clone()))
            .expect("node not registered");

        let del_key = RegistryKey::Delegation(delegator.clone(), node.clone());
        let mut rec: DelegationRecord = env
            .storage()
            .persistent()
            .get(&del_key)
            .expect("no active delegation");
        assert!(rec.amount >= amount, "insufficient delegated amount");

        // Checkpoint rewards on the full amount before reducing it.
        rec.pending_rewards += Self::calc_pending(&rec, m.rewards_per_stake);
        rec.debt_snapshot = m.rewards_per_stake;
        rec.amount -= amount;

        if rec.amount == 0 {
            env.storage().persistent().remove(&del_key);
        } else {
            env.storage().persistent().set(&del_key, &rec);
        }

        // Update node totals.
        m.total_delegated_stake -= amount;
        env.storage()
            .persistent()
            .set(&RegistryKey::Member(node.clone()), &m);

        // Create the pending undelegation.
        let cooldown: u32 = env
            .storage()
            .instance()
            .get(&RegistryKey::DelegationCooldown)
            .unwrap_or(0);
        let unlock_ledger = env.ledger().sequence() + cooldown;
        let pending = UndelegationRequest {
            delegator: delegator.clone(),
            node: node.clone(),
            amount,
            unlock_ledger,
        };
        env.storage().persistent().set(
            &RegistryKey::PendingUndelegation(delegator.clone(), node.clone()),
            &pending,
        );

        env.events().publish(
            (Symbol::new(&env, "undelegation_started"), node),
            (delegator, amount, unlock_ledger),
        );
    }

    /// Withdraw tokens from a completed undelegation once the cooldown has elapsed.
    pub fn withdraw_undelegation(env: Env, delegator: Address, node: Address) -> i128 {
        delegator.require_auth();

        let pend_key = RegistryKey::PendingUndelegation(delegator.clone(), node.clone());
        let pending: UndelegationRequest = env
            .storage()
            .persistent()
            .get(&pend_key)
            .expect("no pending undelegation");

        assert!(
            env.ledger().sequence() >= pending.unlock_ledger,
            "cooldown not elapsed"
        );

        let amount = pending.amount;
        env.storage().persistent().remove(&pend_key);

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::StakeToken)
            .unwrap();
        let token = token::Client::new(&env, &token_addr);
        token.transfer(&env.current_contract_address(), &delegator, &amount);

        env.events().publish(
            (Symbol::new(&env, "undelegation_withdrawn"), node),
            (delegator, amount),
        );

        amount
    }

    /// Claim all pending rewards for a delegator from a specific node.
    /// Returns the amount transferred.
    pub fn claim_rewards(env: Env, delegator: Address, node: Address) -> i128 {
        delegator.require_auth();

        let m: CommitteeMember = env
            .storage()
            .persistent()
            .get(&RegistryKey::Member(node.clone()))
            .expect("node not registered");

        let del_key = RegistryKey::Delegation(delegator.clone(), node.clone());
        let mut rec: DelegationRecord = env
            .storage()
            .persistent()
            .get(&del_key)
            .expect("no active delegation");

        // Settle latest rewards.
        let fresh = Self::calc_pending(&rec, m.rewards_per_stake);
        let total = rec.pending_rewards + fresh;
        assert!(total > 0, "no rewards to claim");

        rec.pending_rewards = 0;
        rec.debt_snapshot = m.rewards_per_stake;
        env.storage().persistent().set(&del_key, &rec);

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::StakeToken)
            .unwrap();
        let token = token::Client::new(&env, &token_addr);
        token.transfer(&env.current_contract_address(), &delegator, &total);

        env.events().publish(
            (Symbol::new(&env, "rewards_claimed"), node),
            (delegator, total),
        );

        total
    }

    /// Distribute `amount` tokens from the caller as fees for a node.
    ///
    /// The node-operator portion (`fee_rate_bps` of `amount`) is transferred
    /// directly to the node address.  The remainder increases
    /// `rewards_per_stake` so delegators can claim it pro-rata.
    ///
    /// If the node has no delegators, the entire amount goes to the node.
    pub fn distribute_fees(env: Env, payer: Address, node: Address, amount: i128) {
        payer.require_auth();
        assert!(amount > 0, "amount must be positive");

        let mut m: CommitteeMember = env
            .storage()
            .persistent()
            .get(&RegistryKey::Member(node.clone()))
            .expect("node not registered");
        assert!(m.active, "node not active");

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::StakeToken)
            .unwrap();
        let token = token::Client::new(&env, &token_addr);
        token.transfer(&payer, &env.current_contract_address(), &amount);

        // Split: node commission vs. delegator share.
        let node_share = amount * m.fee_rate_bps as i128 / 10_000;
        let delegator_total = amount - node_share;

        // Pay the node operator directly.
        if node_share > 0 {
            token.transfer(&env.current_contract_address(), &node, &node_share);
        }

        // Increase rewards_per_stake for all current delegators.
        if delegator_total > 0 && m.total_delegated_stake > 0 {
            m.rewards_per_stake += delegator_total * REWARD_SCALE / m.total_delegated_stake;
        } else if delegator_total > 0 {
            // No delegators — extra goes to the node operator.
            token.transfer(&env.current_contract_address(), &node, &delegator_total);
        }

        env.storage()
            .persistent()
            .set(&RegistryKey::Member(node.clone()), &m);

        env.events().publish(
            (Symbol::new(&env, "fees_distributed"), node),
            (payer, amount, node_share, delegator_total),
        );
    }

    /// Return the active delegation record for (delegator, node), if any.
    pub fn get_delegation(
        env: Env,
        delegator: Address,
        node: Address,
    ) -> Option<DelegationRecord> {
        env.storage()
            .persistent()
            .get(&RegistryKey::Delegation(delegator, node))
    }

    /// Return the pending undelegation request for (delegator, node), if any.
    pub fn get_pending_undelegation(
        env: Env,
        delegator: Address,
        node: Address,
    ) -> Option<UndelegationRequest> {
        env.storage()
            .persistent()
            .get(&RegistryKey::PendingUndelegation(delegator, node))
    }

    /// View unclaimed rewards for a delegator without mutating state.
    pub fn pending_rewards(env: Env, delegator: Address, node: Address) -> i128 {
        let m: CommitteeMember = env
            .storage()
            .persistent()
            .get(&RegistryKey::Member(node.clone()))
            .expect("node not registered");
        let del_key = RegistryKey::Delegation(delegator.clone(), node);
        let rec: DelegationRecord = match env.storage().persistent().get(&del_key) {
            Some(r) => r,
            None => return 0,
        };
        rec.pending_rewards + Self::calc_pending(&rec, m.rewards_per_stake)
    }

    /// Admin can update the delegation cooldown period.
    pub fn set_delegation_cooldown(env: Env, admin: Address, cooldown_ledgers: u32) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&RegistryKey::DelegationCooldown, &cooldown_ledgers);
        env.events().publish(
            (Symbol::new(&env, "cooldown_updated"),),
            cooldown_ledgers,
        );
    }

    /// Compute rewards accrued on `rec` up to `current_rps` without writing.
    fn calc_pending(rec: &DelegationRecord, current_rps: i128) -> i128 {
        let delta = current_rps.saturating_sub(rec.debt_snapshot);
        if delta <= 0 || rec.amount <= 0 {
            return 0;
        }
        rec.amount * delta / REWARD_SCALE
    }

    fn require_admin(env: &Env, admin: &Address) {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::Admin)
            .expect("not initialized");
        assert!(constant_time::address_eq(env, admin, &stored_admin), "not admin");
    }

    fn timeout_for_phase(config: &TimeoutConfig, phase: &GamePhase) -> u32 {
        match phase {
            GamePhase::Deal => config.deal_ledgers,
            GamePhase::Reveal => config.reveal_ledgers,
            GamePhase::Showdown => config.showdown_ledgers,
        }
    }

    fn slash_member_stake(env: &Env, member: &Address, reason: Symbol) -> i128 {
        let mut m: CommitteeMember = env
            .storage()
            .persistent()
            .get(&RegistryKey::Member(member.clone()))
            .expect("not a member");
        m.slash_count += 1;
        let slashed = m.stake / 2;
        m.stake -= slashed;
        m.active = false;

        // Slash delegators proportionally (same 50 % haircut).
        let mut total_delegation_slashed: i128 = 0;
        let delegators: Vec<Address> = env
            .storage()
            .persistent()
            .get(&RegistryKey::NodeDelegators(member.clone()))
            .unwrap_or_else(|| Vec::new(env));
        for i in 0..delegators.len() {
            let delegator = delegators.get(i).unwrap();
            let del_key = RegistryKey::Delegation(delegator.clone(), member.clone());
            if let Some(mut rec) = env.storage().persistent().get::<RegistryKey, DelegationRecord>(&del_key) {
                // Checkpoint rewards before slashing the amount.
                rec.pending_rewards += Self::calc_pending(&rec, m.rewards_per_stake);
                rec.debt_snapshot = m.rewards_per_stake;
                let slash_amount = rec.amount / 2;
                rec.amount -= slash_amount;
                total_delegation_slashed += slash_amount;
                m.total_delegated_stake -= slash_amount;
                env.storage().persistent().set(&del_key, &rec);
            }
            // Also haircut any pending (cooling-down) undelegation.
            let pend_key = RegistryKey::PendingUndelegation(delegator.clone(), member.clone());
            if let Some(mut pend) = env.storage().persistent().get::<RegistryKey, UndelegationRequest>(&pend_key) {
                let slash_amount = pend.amount / 2;
                pend.amount -= slash_amount;
                total_delegation_slashed += slash_amount;
                env.storage().persistent().set(&pend_key, &pend);
            }
        }

        env.events().publish(
            (Symbol::new(env, "slash_reported"), m.slash_count),
            (member.clone(), reason),
        );
        env.storage()
            .persistent()
            .set(&RegistryKey::Member(member.clone()), &m);

        // Total slashed is node stake + delegator stake haircut.
        slashed + total_delegation_slashed
    }

    fn slash_member_record(env: &Env, member: &Address, reason: Symbol) -> CommitteeMember {
        let mut m: CommitteeMember = env
            .storage()
            .persistent()
            .get(&RegistryKey::Member(member.clone()))
            .expect("not a member");

        m.slash_count += 1;
        env.events().publish(
            (Symbol::new(env, "slash_reported"), m.slash_count),
            (member.clone(), reason),
        );

        if m.slash_count >= 3 {
            let slashed = m.stake / 2;
            m.stake -= slashed;
            m.active = false;
        }

        env.storage()
            .persistent()
            .set(&RegistryKey::Member(member.clone()), &m);
        m
    }

    fn redistribute_slashed_stake(env: &Env, players: &Vec<Address>, amount: i128) {
        if amount <= 0 || players.is_empty() {
            return;
        }
        let token_addr: Address = env
            .storage()
            .instance()
            .get(&RegistryKey::StakeToken)
            .unwrap();
        let token = token::Client::new(env, &token_addr);
        let share = amount / players.len() as i128;
        let mut remainder = amount % players.len() as i128;
        for i in 0..players.len() {
            let player = players.get(i).unwrap();
            let odd = if remainder > 0 {
                remainder -= 1;
                1
            } else {
                0
            };
            token.transfer(&env.current_contract_address(), &player, &(share + odd));
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        token::{StellarAssetClient, TokenClient},
    };

    struct Setup<'a> {
        env: Env,
        client: CommitteeRegistryContractClient<'a>,
        token: TokenClient<'a>,
        admin: Address,
        member: Address,
    }

    fn setup() -> Setup<'static> {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CommitteeRegistryContract, ());
        let client = CommitteeRegistryContractClient::new(&env, &contract_id);

        let token_admin_addr = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(token_admin_addr);
        let token = TokenClient::new(&env, &sac.address());
        let token_admin = StellarAssetClient::new(&env, &sac.address());

        let admin = Address::generate(&env);
        let member = Address::generate(&env);
        client.initialize(&admin, &token.address, &1_000, &100);
        token_admin.mint(&member, &2_000);
        client.register_member(
            &member,
            &1_000,
            &soroban_sdk::String::from_str(&env, "node-0"),
            &soroban_sdk::String::from_str(&env, "us-east-1"),
            &1_000, // 10% fee rate
        );

        Setup {
            env,
            client,
            token,
            admin,
            member,
        }
    }

    fn setup_paused() -> (
        Env,
        CommitteeRegistryContractClient<'static>,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CommitteeRegistryContract, ());
        let client = CommitteeRegistryContractClient::new(&env, &contract_id);
        let token_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
        let admin = Address::generate(&env);
        client.initialize(&admin, &sac.address(), &100, &0);
        (env, client, admin)
    }

    #[test]
    fn get_active_members_returns_all_registered() {
        let s = setup();
        let env = &s.env;

        let member2 = Address::generate(env);
        let token_admin = StellarAssetClient::new(env, &s.token.address);
        token_admin.mint(&member2, &2_000);

        s.client.register_member(
            &member2,
            &1_000,
            &soroban_sdk::String::from_str(env, "node-1"),
            &soroban_sdk::String::from_str(env, "eu-west-1"),
            &500, // 5% fee rate
        );

        let active = s.client.get_active_members();
        assert_eq!(active.len(), 2);

        let m1 = active.get(0).unwrap();
        let m2 = active.get(1).unwrap();

        assert_eq!(m1.address, s.member);
        assert_eq!(m1.region, soroban_sdk::String::from_str(env, "us-east-1"));

        assert_eq!(m2.address, member2);
        assert_eq!(m2.region, soroban_sdk::String::from_str(env, "eu-west-1"));
    }

    #[test]
    fn timeout_config_defaults_and_updates() {
        let s = setup();
        let config = s.client.get_timeout_config();
        assert_eq!(config.deal_ledgers, 120);
        assert_eq!(config.reveal_ledgers, 120);
        assert_eq!(config.showdown_ledgers, 120);

        s.client.set_timeout_config(&s.admin, &5, &7, &9);
        let config = s.client.get_timeout_config();
        assert_eq!(config.deal_ledgers, 5);
        assert_eq!(config.reveal_ledgers, 7);
        assert_eq!(config.showdown_ledgers, 9);
    }

    #[test]
    fn report_timeout_slashes_and_redistributes_to_affected_players() {
        let s = setup();
        s.client.set_timeout_config(&s.admin, &2, &4, &6);
        let p1 = Address::generate(&s.env);
        let p2 = Address::generate(&s.env);
        let players = Vec::from_array(&s.env, [p1.clone(), p2.clone()]);
        s.client
            .track_game_phase(&s.admin, &42, &GamePhase::Deal, &players);

        s.env.ledger().with_mut(|ledger| {
            ledger.sequence_number += 2;
        });

        let slashed = s.client.report_timeout(&42, &s.member);
        assert_eq!(slashed, 500);

        let member = s.client.get_member(&s.member);
        assert_eq!(member.stake, 500);
        assert!(!member.active);
        assert_eq!(member.slash_count, 1);
        assert_eq!(s.token.balance(&p1), 250);
        assert_eq!(s.token.balance(&p2), 250);

        let game = s.client.get_game_liveness(&42);
        assert!(game.resolved);
    }

    #[test]
    #[should_panic(expected = "timeout not reached")]
    fn report_timeout_before_window_reverts() {
        let s = setup();
        s.client.set_timeout_config(&s.admin, &10, &10, &10);
        let player = Address::generate(&s.env);
        let players = Vec::from_array(&s.env, [player]);
        s.client
            .track_game_phase(&s.admin, &7, &GamePhase::Showdown, &players);

        s.client.report_timeout(&7, &s.member);
    }
}

#[cfg(test)]
mod test_paused {
    use super::*;
    use soroban_sdk::{
        testutils::Address as _,
        token::{StellarAssetClient, TokenClient},
        Address, Env, String, Vec,
    };

    fn setup_paused() -> (
        Env,
        CommitteeRegistryContractClient<'static>,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CommitteeRegistryContract, ());
        let client = CommitteeRegistryContractClient::new(&env, &contract_id);
        let token_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
        let admin = Address::generate(&env);
        client.initialize(&admin, &sac.address(), &100, &0);
        (env, client, admin)
    }

    fn setup() -> (
        Env,
        CommitteeRegistryContractClient<'static>,
        Address,
        TokenClient<'static>,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CommitteeRegistryContract, ());
        let client = CommitteeRegistryContractClient::new(&env, &contract_id);

        let token_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token = TokenClient::new(&env, &sac.address());
        let token_sac = StellarAssetClient::new(&env, &sac.address());

        let admin = Address::generate(&env);
        client.initialize(&admin, &sac.address(), &100, &0);

        // Mint tokens for a member
        let member = Address::generate(&env);
        token_sac.mint(&member, &1000);
        let _ = member; // avoid unused warning; caller mints their own

        (env, client, admin, token)
    }

    #[test]
    fn test_pause_and_unpause() {
        let (_env, client, admin) = setup_paused();
        assert!(!client.is_paused());
        client.pause(&admin);
        assert!(client.is_paused());
        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    #[test]
    #[should_panic(expected = "contract paused")]
    fn test_paused_blocks_register_member() {
        let (env, client, admin) = setup_paused();
        client.pause(&admin);

        let member = Address::generate(&env);
        let endpoint = String::from_str(&env, "http://node0:8101");
        let region = String::from_str(&env, "us-east-1");
        client.register_member(&member, &500, &endpoint, &region, &0);
    }

    #[test]
    #[should_panic(expected = "contract paused")]
    fn test_paused_blocks_create_epoch() {
        let (env, client, admin) = setup_paused();
        client.pause(&admin);

        let members: Vec<Address> = Vec::new(&env);
        client.create_epoch(&admin, &members, &2);
    }

    #[test]
    fn test_admin_can_read_while_paused() {
        let (_env, client, admin) = setup_paused();
        client.pause(&admin);
        // get_current_epoch is a read and must not panic
        let epoch = client.get_current_epoch();
        assert!(epoch.is_none());
    }

    #[test]
    fn test_unpause_allows_operations_again() {
        let env = Env::default();
        env.mock_all_auths();
        let token_admin = Address::generate(&env);
        let sac2 = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_sac2 = StellarAssetClient::new(&env, &sac2.address());
        let admin2 = Address::generate(&env);
        let contract_id2 = env.register(CommitteeRegistryContract, ());
        let client2 = CommitteeRegistryContractClient::new(&env, &contract_id2);
        client2.initialize(&admin2, &sac2.address(), &100, &0);

        let member = Address::generate(&env);
        token_sac2.mint(&member, &500);

        client2.pause(&admin2);
        client2.unpause(&admin2);

        let endpoint = String::from_str(&env, "http://node0:8101");
        let region = String::from_str(&env, "us-east-1");
        client2.register_member(&member, &500, &endpoint, &region, &0);
        let m = client2.get_member(&member);
        assert!(m.active);
    }

    #[test]
    #[should_panic(expected = "not admin")]
    fn test_non_admin_cannot_pause() {
        let (env, client, _admin) = setup_paused();
        let stranger = Address::generate(&env);
        client.pause(&stranger);
    }
}

#[cfg(test)]
mod test_delegation {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        token::{StellarAssetClient, TokenClient},
        Env, String,
    };

    /// Helper: set up a registry with one node already registered.
    /// Returns (env, client, token, admin, node).
    fn setup() -> (
        Env,
        CommitteeRegistryContractClient<'static>,
        TokenClient<'static>,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(CommitteeRegistryContract, ());
        let client = CommitteeRegistryContractClient::new(&env, &contract_id);

        let token_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token = TokenClient::new(&env, &sac.address());
        let token_sac = StellarAssetClient::new(&env, &sac.address());

        let admin = Address::generate(&env);
        let node = Address::generate(&env);

        // cooldown = 10 ledgers
        client.initialize(&admin, &sac.address(), &1_000, &10);

        token_sac.mint(&node, &5_000);
        // fee_rate_bps = 1000 (10%)
        client.register_member(
            &node,
            &1_000,
            &String::from_str(&env, "node-0"),
            &String::from_str(&env, "us-east-1"),
            &1_000,
        );

        (env, client, token, admin, node)
    }

    #[test]
    fn delegate_and_query() {
        let (env, client, token, _admin, node) = setup();
        let sac = StellarAssetClient::new(&env, &token.address);

        let delegator = Address::generate(&env);
        sac.mint(&delegator, &2_000);

        client.delegate(&delegator, &node, &500);

        let rec = client.get_delegation(&delegator, &node).unwrap();
        assert_eq!(rec.amount, 500);
        assert_eq!(rec.pending_rewards, 0);

        let m = client.get_member(&node);
        assert_eq!(m.total_delegated_stake, 500);
        // Delegator transferred 500; they now hold 1500.
        assert_eq!(token.balance(&delegator), 1_500);
    }

    #[test]
    fn delegate_accumulates_on_second_call() {
        let (env, client, token, _admin, node) = setup();
        let sac = StellarAssetClient::new(&env, &token.address);
        let delegator = Address::generate(&env);
        sac.mint(&delegator, &3_000);

        client.delegate(&delegator, &node, &500);
        client.delegate(&delegator, &node, &300);

        let rec = client.get_delegation(&delegator, &node).unwrap();
        assert_eq!(rec.amount, 800);
        assert_eq!(token.balance(&delegator), 2_200);
    }

    #[test]
    fn distribute_fees_and_claim_rewards() {
        let (env, client, token, _admin, node) = setup();
        let sac = StellarAssetClient::new(&env, &token.address);

        let delegator = Address::generate(&env);
        sac.mint(&delegator, &2_000);
        client.delegate(&delegator, &node, &1_000);

        // Distribute 1000 tokens as fees.
        // node fee = 10% of 1000 = 100  → sent directly to node
        // delegator share = 900
        let payer = Address::generate(&env);
        sac.mint(&payer, &1_000);
        client.distribute_fees(&payer, &node, &1_000);

        // Node operator received 100 directly (node had 4000 remaining after
        // staking 1000, so total balance = 4000 + 100 = 4100).
        assert_eq!(token.balance(&node), 4_100);

        // Delegator should be able to claim ~900 (modulo REWARD_SCALE rounding).
        let pending = client.pending_rewards(&delegator, &node);
        assert_eq!(pending, 900);

        let claimed = client.claim_rewards(&delegator, &node);
        assert_eq!(claimed, 900);
        // Started with 2000, delegated 1000, claimed 900 → 1900.
        assert_eq!(token.balance(&delegator), 1_900);
    }

    #[test]
    fn distribute_fees_no_delegators_all_goes_to_node() {
        let (env, client, token, _admin, node) = setup();
        let sac = StellarAssetClient::new(&env, &token.address);

        let payer = Address::generate(&env);
        sac.mint(&payer, &500);
        client.distribute_fees(&payer, &node, &500);

        // No delegators: all 500 go to the node operator.
        // Node minted 5000, staked 1000 → 4000 remaining + 500 fees = 4500.
        assert_eq!(token.balance(&node), 4_500);
    }

    #[test]
    fn undelegate_starts_cooldown() {
        let (env, client, _token, _admin, node) = setup();
        let sac = StellarAssetClient::new(&env, &_token.address);

        let delegator = Address::generate(&env);
        sac.mint(&delegator, &2_000);
        client.delegate(&delegator, &node, &1_000);

        client.undelegate(&delegator, &node, &400);

        // Active delegation reduced.
        let rec = client.get_delegation(&delegator, &node).unwrap();
        assert_eq!(rec.amount, 600);

        // Pending undelegation created.
        let pend = client.get_pending_undelegation(&delegator, &node).unwrap();
        assert_eq!(pend.amount, 400);
        // unlock_ledger = current (0) + 10 cooldown
        assert_eq!(pend.unlock_ledger, 10);

        // Node total_delegated_stake reduced.
        let m = client.get_member(&node);
        assert_eq!(m.total_delegated_stake, 600);
    }

    #[test]
    #[should_panic(expected = "cooldown not elapsed")]
    fn withdraw_before_cooldown_panics() {
        let (env, client, _token, _admin, node) = setup();
        let sac = StellarAssetClient::new(&env, &_token.address);

        let delegator = Address::generate(&env);
        sac.mint(&delegator, &2_000);
        client.delegate(&delegator, &node, &1_000);
        client.undelegate(&delegator, &node, &400);
        // ledger is still at 0; cooldown is 10 → must panic
        client.withdraw_undelegation(&delegator, &node);
    }

    #[test]
    fn withdraw_after_cooldown_returns_tokens() {
        let (env, client, token, _admin, node) = setup();
        let sac = StellarAssetClient::new(&env, &token.address);

        let delegator = Address::generate(&env);
        sac.mint(&delegator, &2_000);
        client.delegate(&delegator, &node, &1_000);
        client.undelegate(&delegator, &node, &400);

        env.ledger().with_mut(|l| l.sequence_number += 10);
        let returned = client.withdraw_undelegation(&delegator, &node);
        assert_eq!(returned, 400);
        assert_eq!(token.balance(&delegator), 1_400); // started 2000, delegated 1000, got 400 back
        // Pending undelegation gone.
        assert!(client.get_pending_undelegation(&delegator, &node).is_none());
    }

    #[test]
    #[should_panic(expected = "existing undelegation pending; withdraw first")]
    fn second_undelegate_before_withdraw_panics() {
        let (env, client, _token, _admin, node) = setup();
        let sac = StellarAssetClient::new(&env, &_token.address);

        let delegator = Address::generate(&env);
        sac.mint(&delegator, &3_000);
        client.delegate(&delegator, &node, &2_000);
        client.undelegate(&delegator, &node, &500);
        // second call before withdraw must panic
        client.undelegate(&delegator, &node, &500);
    }

    #[test]
    fn slash_propagates_to_delegators() {
        let (env, client, token, admin, node) = setup();
        let sac = StellarAssetClient::new(&env, &token.address);

        let delegator = Address::generate(&env);
        sac.mint(&delegator, &2_000);
        client.delegate(&delegator, &node, &1_000);

        // Trigger a timeout slash via report_timeout path.
        client.set_timeout_config(&admin, &2, &2, &2);
        let player = Address::generate(&env);
        let players = Vec::from_array(&env, [player.clone()]);
        client.track_game_phase(&admin, &1, &GamePhase::Deal, &players);
        env.ledger().with_mut(|l| l.sequence_number += 2);
        client.report_timeout(&1, &node);

        // Node stake halved: 1000 → 500
        let m = client.get_member(&node);
        assert_eq!(m.stake, 500);
        assert!(!m.active);

        // Delegator amount halved: 1000 → 500
        let rec = client.get_delegation(&delegator, &node).unwrap();
        assert_eq!(rec.amount, 500);
    }

    #[test]
    fn set_delegation_cooldown_updates_value() {
        let (env, client, _token, admin, _node) = setup();
        client.set_delegation_cooldown(&admin, &50);

        // Verify by undelegating and checking the new unlock_ledger.
        let sac = StellarAssetClient::new(&env, &_token.address);
        let delegator = Address::generate(&env);
        sac.mint(&delegator, &2_000);
        client.delegate(&delegator, &_node, &1_000);
        client.undelegate(&delegator, &_node, &200);
        let pend = client.get_pending_undelegation(&delegator, &_node).unwrap();
        assert_eq!(pend.unlock_ledger, 50); // 0 + 50
    }
}
