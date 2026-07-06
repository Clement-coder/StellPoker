use serde::{Deserialize, Serialize};

/// Request body for `POST /api/flags/:key`.
#[derive(Deserialize)]
pub struct SetFlagBody {
    pub enabled: bool,
}

#[derive(Deserialize)]
pub struct DealRequest {
    pub players: Vec<String>,
}

#[derive(Serialize)]
pub struct DealResponse {
    pub status: String,
    pub deck_root: String,
    pub hand_commitments: Vec<String>,
    pub proof_size: usize,
    pub session_id: String,
    pub tx_hash: Option<String>,
}

#[derive(Serialize)]
pub struct RevealResponse {
    pub status: String,
    pub cards: Vec<u32>,
    pub proof_size: usize,
    pub session_id: String,
    pub tx_hash: Option<String>,
}

#[derive(Serialize)]
pub struct ShowdownResponse {
    pub status: String,
    pub winner: String,
    pub winner_index: u32,
    pub proof_size: usize,
    pub session_id: String,
    pub tx_hash: Option<String>,
}

#[derive(Deserialize)]
pub struct PlayerActionRequest {
    pub action: String,
    pub amount: Option<i128>,
}

#[derive(Serialize)]
pub struct PlayerActionResponse {
    pub status: String,
    pub action: String,
    pub amount: Option<i128>,
    pub player: String,
    pub tx_hash: Option<String>,
}

#[derive(Serialize)]
pub struct TableStateResponse {
    pub state: String,
}

#[derive(Serialize)]
pub struct PlayerCardsResponse {
    pub card1: u32,
    pub card2: u32,
    pub salt1: String,
    pub salt2: String,
}

#[derive(Serialize)]
pub struct CommitteeStatusResponse {
    pub nodes: usize,
    pub healthy: Vec<bool>,
    pub status: String,
}

#[derive(Deserialize)]
pub struct RegisterNodeRequest {
    /// Stable node identifier (e.g. "0", "1", "2").
    pub id: String,
    /// Base URL the coordinator should use to reach the node.
    pub endpoint: String,
}

#[derive(Serialize)]
pub struct NodeRegistryResponse {
    pub id: String,
    /// Total registered nodes after the operation.
    pub registered: usize,
    /// Number currently considered healthy.
    pub healthy: usize,
}

#[derive(Serialize)]
pub struct ChainConfigResponse {
    pub rpc_url: String,
    pub network_passphrase: String,
    pub poker_table_contract: String,
}

#[derive(Deserialize)]
pub struct CreateTableRequest {
    pub max_players: Option<u32>,
    pub solo: Option<bool>,
    pub buy_in: Option<String>,
    pub region: Option<String>,
}

#[derive(Serialize)]
pub struct CreateTableResponse {
    pub table_id: u32,
    pub max_players: u32,
    pub joined_wallets: usize,
}

#[derive(Serialize)]
pub struct OpenTablesResponse {
    pub tables: Vec<OpenTableInfo>,
}

#[derive(Serialize)]
pub struct OpenTableInfo {
    pub table_id: u32,
    pub phase: String,
    pub max_players: u32,
    pub joined_wallets: usize,
    pub open_wallet_slots: usize,
}

#[derive(Serialize)]
pub struct JoinTableResponse {
    pub table_id: u32,
    pub seat_index: u32,
    pub seat_address: String,
    pub joined_wallets: usize,
    pub max_players: u32,
}

#[derive(Serialize)]
pub struct TableLobbyResponse {
    pub table_id: u32,
    pub phase: String,
    pub max_players: u32,
    pub seats: Vec<LobbySeat>,
    pub joined_wallets: usize,
}

#[derive(Serialize)]
pub struct LobbySeat {
    pub seat_index: u32,
    pub chain_address: String,
    pub wallet_address: Option<String>,
}

#[derive(Deserialize)]
pub struct WalletChallengeRequest {
    pub address: String,
}

#[derive(Serialize)]
pub struct WalletChallengeResponse {
    pub challenge: String,
}

#[derive(Deserialize)]
pub struct WalletVerifyRequest {
    pub address: String,
    pub challenge: String,
    pub signature: String,
}

#[derive(Serialize)]
pub struct WalletVerifyResponse {
    pub verified: bool,
}

