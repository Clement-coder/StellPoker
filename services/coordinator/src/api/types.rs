use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Per-node MPC phase progress for a specific table, returned by
/// `GET /api/table/:table_id/mpc-status` and included in WebSocket pushes.
#[derive(Serialize, Clone, ToSchema)]
pub struct MpcNodeProgress {
    pub endpoint: String,
    pub phase: String,
    pub healthy: bool,
    pub elapsed_secs: u64,
}

#[derive(Serialize, ToSchema)]
pub struct TableMpcStatusResponse {
    pub table_id: u32,
    pub phase: String,
    pub nodes: Vec<MpcNodeProgress>,
    pub active_sessions: usize,
}

/// Request body for `POST /api/flags/:key`.
#[derive(Deserialize, ToSchema)]
pub struct SetFlagBody {
    pub enabled: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct DealRequest {
    pub players: Vec<String>,
    #[serde(default = "default_circuit")]
    pub circuit_name: String,
}

fn default_circuit() -> String {
    "deal_valid".to_string()
}

#[derive(Serialize, ToSchema)]
pub struct DealResponse {
    pub status: String,
    pub deck_root: String,
    pub hand_commitments: Vec<String>,
    pub proof_size: usize,
    pub session_id: String,
    pub tx_hash: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct RevealResponse {
    pub status: String,
    pub cards: Vec<u32>,
    pub proof_size: usize,
    pub session_id: String,
    pub tx_hash: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ShowdownResponse {
    pub status: String,
    pub winner: String,
    pub winner_index: u32,
    pub proof_size: usize,
    pub session_id: String,
    pub tx_hash: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct PlayerActionRequest {
    pub action: String,
    pub amount: Option<i128>,
}

#[derive(Serialize, ToSchema)]
pub struct PlayerActionResponse {
    pub status: String,
    pub action: String,
    pub amount: Option<i128>,
    pub player: String,
    pub tx_hash: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct TableStateResponse {
    pub state: String,
}

#[derive(Serialize, ToSchema)]
pub struct PlayerCardsResponse {
    pub card1: u32,
    pub card2: u32,
    pub salt1: String,
    pub salt2: String,
}

#[derive(Serialize, ToSchema)]
pub struct CommitteeStatusResponse {
    pub nodes: usize,
    pub healthy: Vec<bool>,
    pub status: String,
}

#[derive(Deserialize, ToSchema)]
pub struct RegisterNodeRequest {
    /// Stable node identifier (e.g. "0", "1", "2").
    pub id: String,
    /// Base URL the coordinator should use to reach the node.
    pub endpoint: String,
}

#[derive(Serialize, ToSchema)]
pub struct NodeRegistryResponse {
    pub id: String,
    /// Total registered nodes after the operation.
    pub registered: usize,
    /// Number currently considered healthy.
    pub healthy: usize,
}

#[derive(Serialize, ToSchema)]
pub struct ChainConfigResponse {
    pub rpc_url: String,
    pub network_passphrase: String,
    pub poker_table_contract: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateTableRequest {
    pub max_players: Option<u32>,
    pub solo: Option<bool>,
    pub buy_in: Option<String>,
    pub region: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateTableResponse {
    pub table_id: u32,
    pub max_players: u32,
    pub joined_wallets: usize,
}

#[derive(Serialize, ToSchema)]
pub struct OpenTablesResponse {
    pub tables: Vec<OpenTableInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct OpenTableInfo {
    pub table_id: u32,
    pub phase: String,
    pub max_players: u32,
    pub joined_wallets: usize,
    pub open_wallet_slots: usize,
}

/// Multi-table overview entry for the mini-map (Issue #53).
#[derive(Serialize, ToSchema, Clone)]
pub struct TableOverviewInfo {
    pub table_id: u32,
    pub phase: String,
    pub max_players: u32,
    pub seated: usize,
    pub total_chips: i64,
    pub stacks: Vec<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct TableOverviewResponse {
    pub tables: Vec<TableOverviewInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct JoinTableResponse {
    pub table_id: u32,
    pub seat_index: u32,
    pub seat_address: String,
    pub joined_wallets: usize,
    pub max_players: u32,
}

#[derive(Serialize, ToSchema)]
pub struct TableLobbyResponse {
    pub table_id: u32,
    pub phase: String,
    pub max_players: u32,
    pub seats: Vec<LobbySeat>,
    pub joined_wallets: usize,
}

#[derive(Serialize, ToSchema)]
pub struct LobbySeat {
    pub seat_index: u32,
    pub chain_address: String,
    pub wallet_address: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct RitOptInRequest {
    pub opt_in: bool,
}

#[derive(Serialize, ToSchema)]
pub struct RitOptInResponse {
    pub status: String,
    pub tx_hash: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct WalletChallengeRequest {
    pub address: String,
}

#[derive(Serialize, ToSchema)]
pub struct WalletChallengeResponse {
    pub challenge: String,
}

#[derive(Deserialize, ToSchema)]
pub struct WalletVerifyRequest {
    pub address: String,
    pub challenge: String,
    pub signature: String,
}

#[derive(Serialize, ToSchema)]
pub struct WalletVerifyResponse {
    pub verified: bool,
}

