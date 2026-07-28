//! Tests for the waiting-list / queue system: joining a full table queues
//! instead of erroring, buy-in is escrowed immediately, and a vacated seat
//! auto-seats the next queued player.

#[cfg(test)]
mod queue_test {
    use crate::types::*;
    use crate::{PokerTableContract, PokerTableContractClient};
    use soroban_sdk::{
        contract, contractimpl,
        testutils::Address as _,
        token::{StellarAssetClient, TokenClient},
        Address, Env,
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
        token: TokenClient<'a>,
        token_admin_client: StellarAssetClient<'a>,
        admin: Address,
        committee: Address,
        verifier: Address,
    }

    fn setup() -> Setup<'static> {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(PokerTableContract, ());
        let client = PokerTableContractClient::new(&env, &contract_id);

        let token_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(token_admin);
        let token = TokenClient::new(&env, &sac.address());
        let token_admin_client = StellarAssetClient::new(&env, &sac.address());

        let admin = Address::generate(&env);
        let committee = Address::generate(&env);
        let verifier = env.register(crate::verifier::ZkVerifierContract, ());

        Setup {
            env,
            client,
            token,
            token_admin_client,
            admin,
            committee,
            verifier,
        }
    }

    /// A 2-max-player table, small enough that "full" is trivial to reach.
    fn create_small_table(s: &Setup) -> u32 {
        let game_hub = s.env.register(GameHubContract, ());
        let config = TableConfig {
            token: s.token.address.clone(),
            min_buy_in: 100,
            max_buy_in: 1000,
            blinds_schedule: BlindsSchedule::fixed(&s.env, 5, 10),
            min_players: 2,
            max_players: 2,
            timeout_ledgers: 100,
            committee: s.committee.clone(),
            verifier: s.verifier.clone(),
            game_hub,
            rake_bps: 0,
            max_rebuys: 0,
            jackpot_rake_share_bps: 0,
            min_bad_beat_category: 7,
            min_bad_beat_rank: 12,
        };
        s.client.create_table(&s.admin, &config)
    }

    fn join(s: &Setup, table_id: u32, player: &Address, buy_in: i128) -> u32 {
        s.token_admin_client.mint(player, &buy_in);
        s.client.join_table(&table_id, player, &buy_in)
    }

    #[test]
    fn join_full_table_enqueues_and_escrows_buy_in() {
        let s = setup();
        let table_id = create_small_table(&s);

        let p1 = Address::generate(&s.env);
        let p2 = Address::generate(&s.env);
        join(&s, table_id, &p1, 500);
        join(&s, table_id, &p2, 500);

        let p3 = Address::generate(&s.env);
        let position = join(&s, table_id, &p3, 300);
        assert_eq!(position, 0); // first (and only) entry in the queue

        let queue = s.client.get_queue(&table_id);
        assert_eq!(queue.len(), 1);
        let entry = queue.get(0).unwrap();
        assert_eq!(entry.player, p3);
        assert_eq!(entry.buy_in, 300);

        // Buy-in already escrowed: p3's token balance dropped by 300, and the
        // table is still exactly 2 seated (not 3).
        assert_eq!(s.token.balance(&p3), 0);
        assert_eq!(s.client.get_table(&table_id).players.len(), 2);
    }

    #[test]
    fn leave_table_auto_seats_next_queued_player() {
        let s = setup();
        let table_id = create_small_table(&s);

        let p1 = Address::generate(&s.env);
        let p2 = Address::generate(&s.env);
        join(&s, table_id, &p1, 500);
        join(&s, table_id, &p2, 500);

        let p3 = Address::generate(&s.env);
        join(&s, table_id, &p3, 300);

        // p1 leaves (table is in Waiting phase, so this is allowed).
        s.client.leave_table(&table_id, &p1);

        // p3 should now be seated with exactly their escrowed buy-in as stack.
        let table = s.client.get_table(&table_id);
        assert_eq!(table.players.len(), 2);
        let mut found_p3_stack: Option<i128> = None;
        for i in 0..table.players.len() {
            let p = table.players.get(i).unwrap();
            if p.address == p3 {
                found_p3_stack = Some(p.stack);
            }
        }
        assert_eq!(found_p3_stack, Some(300));

        // Queue is now empty.
        assert_eq!(s.client.get_queue(&table_id).len(), 0);
    }

    #[test]
    fn leave_queue_refunds_escrowed_buy_in() {
        let s = setup();
        let table_id = create_small_table(&s);

        let p1 = Address::generate(&s.env);
        let p2 = Address::generate(&s.env);
        join(&s, table_id, &p1, 500);
        join(&s, table_id, &p2, 500);

        let p3 = Address::generate(&s.env);
        join(&s, table_id, &p3, 300);
        assert_eq!(s.token.balance(&p3), 0);

        let refunded = s.client.leave_queue(&table_id, &p3);
        assert_eq!(refunded, 300);
        assert_eq!(s.token.balance(&p3), 300);
        assert_eq!(s.client.get_queue(&table_id).len(), 0);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #44)")] // AlreadyQueued
    fn join_table_rejects_duplicate_queue_entry() {
        let s = setup();
        let table_id = create_small_table(&s);

        let p1 = Address::generate(&s.env);
        let p2 = Address::generate(&s.env);
        join(&s, table_id, &p1, 500);
        join(&s, table_id, &p2, 500);

        let p3 = Address::generate(&s.env);
        join(&s, table_id, &p3, 300);
        // Second attempt while still queued must fail, not create a
        // duplicate entry.
        s.token_admin_client.mint(&p3, &300);
        s.client.join_table(&table_id, &p3, &300);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #45)")] // NotQueued
    fn leave_queue_rejects_non_queued_player() {
        let s = setup();
        let table_id = create_small_table(&s);
        let stranger = Address::generate(&s.env);
        s.client.leave_queue(&table_id, &stranger);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #46)")] // QueueFull
    fn join_table_rejects_beyond_max_queue_size() {
        let s = setup();
        let table_id = create_small_table(&s);

        let p1 = Address::generate(&s.env);
        let p2 = Address::generate(&s.env);
        join(&s, table_id, &p1, 500);
        join(&s, table_id, &p2, 500);

        // Fill the queue to its cap (MAX_QUEUE_SIZE = 12).
        for _ in 0..12 {
            let p = Address::generate(&s.env);
            join(&s, table_id, &p, 300);
        }

        // The 13th would-be queuer must be rejected.
        let overflow = Address::generate(&s.env);
        join(&s, table_id, &overflow, 300);
    }
}
