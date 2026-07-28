//! Tests for the admin-only propose/execute/cancel contract-upgrade
//! mechanism: minimum timelock delay enforcement, and the commit-reveal
//! hash guarantee (execute always uses the hash committed at propose time).

#[cfg(test)]
mod upgrade_test {
    use crate::types::*;
    use crate::{PokerTableContract, PokerTableContractClient};
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger as _},
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    #[contract]
    pub struct GameHubContract;

    #[contractimpl]
    impl GameHubContract {
        pub fn start_game(
            _env: Env,
            _game_id: Address,
            _session_id: u32,
            _p1: Address,
            _p2: Address,
            _p1_pts: i128,
            _p2_pts: i128,
        ) {
        }
        pub fn end_game(_env: Env, _session_id: u32, _p1_won: bool) {}
    }

    struct Setup<'a> {
        env: Env,
        client: PokerTableContractClient<'a>,
        table_id: u32,
    }

    fn setup() -> Setup<'static> {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(PokerTableContract, ());
        let client = PokerTableContractClient::new(&env, &contract_id);

        let token_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(token_admin);
        let _token_admin_client = StellarAssetClient::new(&env, &sac.address());

        let admin = Address::generate(&env);
        let committee = Address::generate(&env);
        let verifier = env.register(crate::verifier::ZkVerifierContract, ());
        let game_hub = env.register(GameHubContract, ());

        let config = TableConfig {
            token: sac.address(),
            min_buy_in: 100,
            max_buy_in: 1000,
            blinds_schedule: BlindsSchedule::fixed(&env, 5, 10),
            min_players: 2,
            max_players: 6,
            timeout_ledgers: 100,
            committee,
            verifier,
            game_hub,
            rake_bps: 0,
            max_rebuys: 0,
            jackpot_rake_share_bps: 0,
            min_bad_beat_category: 7,
            min_bad_beat_rank: 12,
        };
        let table_id = client.create_table(&admin, &config);

        Setup {
            env,
            client,
            table_id,
        }
    }

    fn fake_hash(env: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(env, &[seed; 32])
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #49)")] // UpgradeDelayTooShort
    fn propose_upgrade_rejects_delay_below_minimum() {
        let s = setup();
        let hash = fake_hash(&s.env, 7);
        // One day minus one second — just under MIN_UPGRADE_DELAY_SECONDS.
        s.client.propose_upgrade(&s.table_id, &hash, &86_399);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #47)")] // NoUpgradeProposal
    fn execute_upgrade_rejects_without_proposal() {
        let s = setup();
        s.client.execute_upgrade(&s.table_id);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #47)")] // NoUpgradeProposal
    fn cancel_upgrade_rejects_without_proposal() {
        let s = setup();
        s.client.cancel_upgrade(&s.table_id);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #48)")] // UpgradeDelayNotElapsed
    fn execute_upgrade_rejects_before_delay_elapses() {
        let s = setup();
        let hash = fake_hash(&s.env, 7);
        s.client.propose_upgrade(&s.table_id, &hash, &86_400);
        // No time has passed — must not be executable yet.
        s.client.execute_upgrade(&s.table_id);
    }

    #[test]
    fn execute_upgrade_rejects_right_up_until_the_delay_boundary() {
        let s = setup();
        let hash = fake_hash(&s.env, 7);
        s.client.propose_upgrade(&s.table_id, &hash, &86_400);

        // One second before the deadline: still not executable.
        s.env.ledger().set_timestamp(86_399);
        let result = s.client.try_execute_upgrade(&s.table_id);
        assert!(result.is_err());
    }

    #[test]
    fn propose_then_cancel_clears_the_pending_proposal() {
        let s = setup();
        let hash = fake_hash(&s.env, 3);
        s.client.propose_upgrade(&s.table_id, &hash, &90_000);

        let proposal = s.client.get_upgrade_proposal(&s.table_id).unwrap();
        assert_eq!(proposal.new_wasm_hash, hash);
        assert_eq!(proposal.execute_after, 90_000);

        s.client.cancel_upgrade(&s.table_id);
        assert_eq!(s.client.get_upgrade_proposal(&s.table_id), None);
    }

    #[test]
    fn a_new_proposal_replaces_the_previous_one() {
        let s = setup();
        let hash_a = fake_hash(&s.env, 1);
        let hash_b = fake_hash(&s.env, 2);

        s.client.propose_upgrade(&s.table_id, &hash_a, &90_000);
        s.client.propose_upgrade(&s.table_id, &hash_b, &100_000);

        let proposal = s.client.get_upgrade_proposal(&s.table_id).unwrap();
        // The later proposal's hash and delay win — no trace of hash_a's
        // commitment lingers for `execute_upgrade` to fall back on.
        assert_eq!(proposal.new_wasm_hash, hash_b);
        assert_eq!(proposal.execute_after, 100_000);
    }

    #[test]
    fn get_upgrade_proposal_is_none_when_nothing_pending() {
        let s = setup();
        assert_eq!(s.client.get_upgrade_proposal(&s.table_id), None);
    }
}
