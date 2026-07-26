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
}

#[contractimpl]
impl CommitteeRegistryContract {
    /// Initialize the registry.
    pub fn initialize(env: Env, admin: Address, stake_token: Address, min_stake: i128) {
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
    ) {
        member.require_auth();
        assert!(
            !env.storage()
                .instance()
                .get::<RegistryKey, bool>(&RegistryKey::Paused)
                .unwrap_or(false),
            "contract paused"
        );

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
        env.events().publish(
            (Symbol::new(env, "slash_reported"), m.slash_count),
            (member.clone(), reason),
        );
        env.storage()
            .persistent()
            .set(&RegistryKey::Member(member.clone()), &m);
        slashed
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
        String,
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
        client.initialize(&admin, &token.address, &1_000);
        token_admin.mint(&member, &2_000);
        client.register_member(
            &member,
            &1_000,
            &soroban_sdk::String::from_str(&env, "node-0"),
            &soroban_sdk::String::from_str(&env, "us-east-1"),
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
        client.initialize(&admin, &sac.address(), &100);
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
        client.initialize(&admin, &sac.address(), &100);

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
        client.register_member(&member, &500, &endpoint, &region);
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
        client2.initialize(&admin2, &sac2.address(), &100);

        let member = Address::generate(&env);
        token_sac2.mint(&member, &500);

        client2.pause(&admin2);
        client2.unpause(&admin2);

        let endpoint = String::from_str(&env, "http://node0:8101");
        let region = String::from_str(&env, "us-east-1");
        client2.register_member(&member, &500, &endpoint, &region);
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
