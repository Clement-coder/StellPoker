#[cfg(test)]
mod token_support_test {
    use crate::types::*;
    use crate::{PokerTableContractClient, PokerTableContract};
    use soroban_sdk::{Address, BytesN, Env, Vec};
    use soroban_sdk::token::{StellarAssetClient, TokenClient};

    // Reuse helper from test.rs by duplicating minimal setup here to avoid cross-module
    fn create_token<'a>(env: &Env, admin: &Address) -> (TokenClient<'a>, StellarAssetClient<'a>) {
        let sac = env.register_stellar_asset_contract_v2(admin.clone());
        (
            TokenClient::new(env, &sac.address()),
            StellarAssetClient::new(env, &sac.address()),
        )
    }

    fn default_config(env: &Env, token: &Address, committee: &Address, verifier: &Address) -> TableConfig {
        let game_hub = env.register(crate::test::GameHubContract, ());
        TableConfig {
            token: token.clone(),
            min_buy_in: 100,
            max_buy_in: 1000,
            small_blind: 5,
            big_blind: 10,
            min_players: 2,
            max_players: 6,
            timeout_ledgers: 100,
            committee: committee.clone(),
            verifier: verifier.clone(),
            game_hub,
            rake_bps: 0,
            max_rebuys: 0,
            jackpot_rake_share_bps: 0,
            min_bad_beat_category: 7,
            min_bad_beat_rank: 12,
        }
    }

    #[test]
    fn test_create_table_with_custom_sac() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(PokerTableContract, ());
        let client = PokerTableContractClient::new(&env, &contract_id);

        let token_admin = Address::generate(&env);
        let (token, token_admin_client) = create_token(&env, &token_admin);

        let admin = Address::generate(&env);
        let committee = Address::generate(&env);
        let verifier = env.register(crate::verifier::ZkVerifierContract, ());

        let config = default_config(&env, &token.address(), &committee, &verifier);
        let table_id = client.create_table(&admin, &config);

        let table = client.get_table(&table_id);
        assert_eq!(table.config.token, token.address());
        assert_eq!(table.config.min_buy_in, 100);
    }

    #[test]
    fn test_join_table_escrow_with_sac() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(PokerTableContract, ());
        let client = PokerTableContractClient::new(&env, &contract_id);

        let token_admin = Address::generate(&env);
        let (token, token_admin_client) = create_token(&env, &token_admin);

        let admin = Address::generate(&env);
        let committee = Address::generate(&env);
        let verifier = env.register(crate::verifier::ZkVerifierContract, ());

        let config = default_config(&env, &token.address(), &committee, &verifier);
        let table_id = client.create_table(&admin, &config);

        let player = Address::generate(&env);
        token_admin_client.mint(&player, &500);
        let seat = client.join_table(&table_id, &player, &500);
        assert_eq!(seat, 0);

        let table = client.get_table(&table_id);
        let p = table.players.get(0).unwrap();
        assert_eq!(p.stack, 500);
    }
}
