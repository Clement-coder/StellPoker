#![no_std]
#![allow(deprecated)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, xdr::ToXdr, Address, Bytes, BytesN, Env,
    Symbol, Vec,
};
use ultrahonk_soroban_verifier::{UltraHonkVerifier, PROOF_BYTES};

/// Public input layout for each circuit type (field element positions):
///
/// DealValid  (20 fields = 640 bytes):
///   [0]  num_players
///   [1]  deck_root
///   [2..8)  hand_commitments[0..6]
///   [8..14) dealt_card1_indices[0..6]
///   [14..20) dealt_card2_indices[0..6]
///
/// RevealBoardValid  (25 fields = 800 bytes):
///   [0]  deck_root
///   [1]  num_revealed
///   [2]  num_previously_used
///   [3..19) previously_used_indices[0..16]
///   [19..22) revealed_cards[0..3]
///   [22..25) revealed_indices[0..3]
///
/// ShowdownValid  (27 fields = 864 bytes):
///   [0]  num_active_players
///   [1..7)  hand_commitments[0..6]
///   [7..12) board_indices[0..5]
///   [12] deck_root
///   [13..19) hole_card1[0..6]
///   [19..25) hole_card2[0..6]
///   [25] winner_index
///   [26] tie_mask

const SHOWDOWN_FIELD_COUNT: u32 = 27;
const SHOWDOWN_BYTES: u32 = SHOWDOWN_FIELD_COUNT * 32;

const DEAL_FIELD_COUNT: u32 = 20;
const DEAL_BYTES: u32 = DEAL_FIELD_COUNT * 32;

const REVEAL_FIELD_COUNT: u32 = 25;
const REVEAL_BYTES: u32 = REVEAL_FIELD_COUNT * 32;

const MAX_PLAYERS: u32 = 6;
const BOARD_INDICES_COUNT: u32 = 5;

fn ct_bytes32_eq(left: &BytesN<32>, right: &BytesN<32>) -> bool {
    let left_arr = left.to_array();
    let right_arr = right.to_array();
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= left_arr[i] ^ right_arr[i];
    }
    diff == 0
}

fn ct_address_eq(env: &Env, left: &Address, right: &Address) -> bool {
    let left_hash: BytesN<32> = env.crypto().keccak256(&left.to_xdr(env)).into();
    let right_hash: BytesN<32> = env.crypto().keccak256(&right.to_xdr(env)).into();
    ct_bytes32_eq(&left_hash, &right_hash)
}

fn ct_u32_eq(left: u32, right: u32) -> bool {
    (left ^ right) == 0
}

/// ZK Verifier contract for Stellar Poker.
///
/// Uses UltraHonk proof verification via Soroban's native BN254 host functions
/// (Protocol 25 / X-Ray). Each circuit type has its own verification key (VK)
/// stored on-chain. Proofs are verified against their circuit's VK and the
/// provided public inputs.
#[contract]
pub struct ZkVerifierContract;

#[contracterror]
#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VerifierError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAdmin = 3,
    NoVkForCircuit = 4,
    VkParseError = 5,
    ProofSizeError = 6,
    VerificationFailed = 7,
    PublicInputSizeError = 8,
    PublicInputMismatch = 9,
    WrongCommitmentCount = 10,
    WrongBoardIndicesCount = 11,
    ContractPaused = 12,
}

#[contracttype]
#[derive(Clone)]
pub enum CircuitType {
    DealValid,
    RevealBoardValid,
    ShowdownValid,
}

#[contracttype]
#[derive(Clone)]
pub enum StorageKey {
    Admin,
    Vk(CircuitType),
    ProofVerified(BytesN<32>),
    Paused,
}

#[contractimpl]
impl ZkVerifierContract {
    /// Initialize the verifier with an admin.
    pub fn initialize(env: Env, admin: Address) -> Result<(), VerifierError> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(VerifierError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&StorageKey::Admin, &admin);
        Ok(())
    }

    /// Pause the verifier (admin only). All proof-verification calls revert while paused.
    /// NOTE: for production consider a timelock or multi-sig for unpause.
    pub fn pause(env: Env, admin: Address) -> Result<(), VerifierError> {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(VerifierError::NotInitialized)?;
        if !ct_address_eq(&env, &admin, &stored_admin) {
            return Err(VerifierError::NotAdmin);
        }
        env.storage().instance().set(&StorageKey::Paused, &true);
        env.events()
            .publish((Symbol::new(&env, "verifier_paused"),), admin);
        Ok(())
    }

    /// Unpause the verifier (admin only).
    /// NOTE: for production consider a timelock or multi-sig here.
    pub fn unpause(env: Env, admin: Address) -> Result<(), VerifierError> {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(VerifierError::NotInitialized)?;
        if !ct_address_eq(&env, &admin, &stored_admin) {
            return Err(VerifierError::NotAdmin);
        }
        env.storage().instance().set(&StorageKey::Paused, &false);
        env.events()
            .publish((Symbol::new(&env, "verifier_unpaused"),), admin);
        Ok(())
    }

    /// Returns true if the verifier is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<StorageKey, bool>(&StorageKey::Paused)
            .unwrap_or(false)
    }

    /// Store a verification key for a circuit type.
    /// Called once per circuit during deployment.
    pub fn set_verification_key(
        env: Env,
        admin: Address,
        circuit: CircuitType,
        vk_data: Bytes,
    ) -> Result<(), VerifierError> {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(VerifierError::NotInitialized)?;
        if !ct_address_eq(&env, &admin, &stored_admin) {
            return Err(VerifierError::NotAdmin);
        }

        // Validate the VK can be parsed before storing
        UltraHonkVerifier::new(&env, &vk_data).map_err(|_| VerifierError::VkParseError)?;

        env.storage()
            .persistent()
            .set(&StorageKey::Vk(circuit.clone()), &vk_data);

        env.events()
            .publish((Symbol::new(&env, "vk_set"),), circuit);
        Ok(())
    }

    /// Verify an UltraHonk proof for a given circuit type.
    ///
    /// 1. Loads the VK for the circuit type
    /// 2. Validates proof size (14,624 bytes = 457 fields * 32)
    /// 3. Runs full UltraHonk verification (sumcheck + shplonk pairing)
    /// 4. Stores proof hash for auditability
    pub fn verify_proof(
        env: Env,
        circuit: CircuitType,
        proof: Bytes,
        public_inputs: Bytes,
    ) -> Result<bool, VerifierError> {
        // Reject all verification calls while paused
        if env
            .storage()
            .instance()
            .get::<StorageKey, bool>(&StorageKey::Paused)
            .unwrap_or(false)
        {
            return Err(VerifierError::ContractPaused);
        }

        // Check proof size
        if proof.len() as usize != PROOF_BYTES {
            return Err(VerifierError::ProofSizeError);
        }

        // Load VK for this circuit
        let vk_bytes: Bytes = env
            .storage()
            .persistent()
            .get(&StorageKey::Vk(circuit))
            .ok_or(VerifierError::NoVkForCircuit)?;

        // Parse VK and create verifier
        let verifier =
            UltraHonkVerifier::new(&env, &vk_bytes).map_err(|_| VerifierError::VkParseError)?;

        // Run full UltraHonk verification
        verifier
            .verify(&proof, &public_inputs)
            .map_err(|_| VerifierError::VerificationFailed)?;

        // Store proof hash for auditability
        let proof_hash = env.crypto().keccak256(&proof);
        env.storage()
            .persistent()
            .set(&StorageKey::ProofVerified(proof_hash.clone().into()), &true);

        env.events()
            .publish((Symbol::new(&env, "proof_verified"),), proof_hash);

        Ok(true)
    }

    /// Check if a proof was previously verified.
    pub fn is_proof_verified(env: Env, proof_hash: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .get(&StorageKey::ProofVerified(proof_hash))
            .unwrap_or(false)
    }

    // ====================================================================
    // Helpers — parse and validate public inputs
    // ====================================================================

    /// Check that a 32-byte field element in `public_inputs` at `field_index`
    /// matches an `expected` BytesN<32>.
    fn check_bytes32_field(public_inputs: &Bytes, field_index: u32, expected: &BytesN<32>) -> bool {
        let start = field_index * 32;
        let expected_arr = expected.to_array();
        let mut diff = 0u8;
        for i in 0..32u32 {
            let actual = public_inputs.get(start + i).unwrap_or(0);
            diff |= actual ^ expected_arr[i as usize];
        }
        diff == 0
    }

    /// Extract a u32 from a BN254 field element at `field_index` in public_inputs.
    /// A small integer value is stored as big-endian in the last 4 bytes of the
    /// 32-byte field element.
    fn extract_u32_field(public_inputs: &Bytes, field_index: u32) -> u32 {
        let start = field_index * 32 + 28;
        let b0 = public_inputs.get(start).unwrap_or(0);
        let b1 = public_inputs.get(start + 1).unwrap_or(0);
        let b2 = public_inputs.get(start + 2).unwrap_or(0);
        let b3 = public_inputs.get(start + 3).unwrap_or(0);
        (b0 as u32) << 24 | (b1 as u32) << 16 | (b2 as u32) << 8 | b3 as u32
    }

    /// Check that a u32 value matches the field element at `field_index`.
    fn check_u32_field(public_inputs: &Bytes, field_index: u32, expected: u32) -> bool {
        ct_u32_eq(Self::extract_u32_field(public_inputs, field_index), expected)
    }

    // ====================================================================
    // Deal proof — validate deck_root and hand_commitments match proof
    // ====================================================================

    /// Verify a deal proof and validate that the proved deck_root and
    /// hand_commitments match the supplied values (which get stored on-chain).
    ///
    /// Public output layout (field indices within public_inputs):
    ///   [0]  num_players
    ///   [1]  deck_root
    ///   [2..8)  hand_commitments[0..6]
    pub fn verify_deal(
        env: Env,
        proof: Bytes,
        public_inputs: Bytes,
        deck_root: BytesN<32>,
        hand_commitments: Vec<BytesN<32>>,
    ) -> Result<bool, VerifierError> {
        if public_inputs.len() != DEAL_BYTES {
            return Err(VerifierError::PublicInputSizeError);
        }
        if hand_commitments.len() > MAX_PLAYERS {
            return Err(VerifierError::WrongCommitmentCount);
        }

        // deck_root at field index 1
        if !Self::check_bytes32_field(&public_inputs, 1, &deck_root) {
            return Err(VerifierError::PublicInputMismatch);
        }

        // hand_commitments at field indices 2 .. 2 + len
        for i in 0..hand_commitments.len() {
            let expected = hand_commitments
                .get(i)
                .ok_or(VerifierError::PublicInputMismatch)?;
            if !Self::check_bytes32_field(&public_inputs, 2 + i, &expected) {
                return Err(VerifierError::PublicInputMismatch);
            }
        }

        Self::verify_proof(env, CircuitType::DealValid, proof, public_inputs)
    }

    // ====================================================================
    // Reveal proof — validate deck_root, revealed cards, and indices
    // ====================================================================

    /// Verify a board reveal proof and validate that the proved deck_root,
    /// revealed card values, and revealed indices match the supplied values.
    ///
    /// Public input/output layout:
    ///   [0]  deck_root  (public input)
    ///   [19..22) revealed_cards[0..3]  (public output)
    ///   [22..25) revealed_indices[0..3]  (public output)
    pub fn verify_reveal(
        env: Env,
        proof: Bytes,
        public_inputs: Bytes,
        deck_root: BytesN<32>,
        revealed_cards: Vec<u32>,
        revealed_indices: Vec<u32>,
    ) -> Result<bool, VerifierError> {
        if public_inputs.len() != REVEAL_BYTES {
            return Err(VerifierError::PublicInputSizeError);
        }
        let num_revealed = revealed_cards.len();
        if num_revealed != revealed_indices.len() || num_revealed > 3 {
            return Err(VerifierError::PublicInputMismatch);
        }

        // deck_root at field index 0 (public input)
        if !Self::check_bytes32_field(&public_inputs, 0, &deck_root) {
            return Err(VerifierError::PublicInputMismatch);
        }

        // revealed_cards at field indices 19 .. 19 + num_revealed
        for i in 0..num_revealed {
            let expected = revealed_cards
                .get(i)
                .ok_or(VerifierError::PublicInputMismatch)?;
            if !Self::check_u32_field(&public_inputs, 19 + i, expected) {
                return Err(VerifierError::PublicInputMismatch);
            }
        }

        // revealed_indices at field indices 22 .. 22 + num_revealed
        for i in 0..num_revealed {
            let expected = revealed_indices
                .get(i)
                .ok_or(VerifierError::PublicInputMismatch)?;
            if !Self::check_u32_field(&public_inputs, 22 + i, expected) {
                return Err(VerifierError::PublicInputMismatch);
            }
        }

        Self::verify_proof(env, CircuitType::RevealBoardValid, proof, public_inputs)
    }

    // ====================================================================
    // Showdown proof — validate hand_commitments, board_indices, deck_root,
    // and return the proved winner_index / tie_mask outputs
    // ====================================================================

    /// Verify a showdown proof and validate that all game-state parameters
    /// (hand_commitments, board_indices, deck_root) match the on-chain state.
    ///
    /// Public input/output layout:
    ///   [0]  num_active_players
    ///   [1..7)  hand_commitments[0..6]
    ///   [7..12) board_indices[0..5]
    ///   [12] deck_root
    ///   [25] winner_index
    ///   [26] tie_mask
    pub fn verify_showdown(
        env: Env,
        proof: Bytes,
        public_inputs: Bytes,
        hand_commitments: Vec<BytesN<32>>,
        board_indices: Vec<u32>,
        deck_root: BytesN<32>,
    ) -> Result<bool, VerifierError> {
        if public_inputs.len() != SHOWDOWN_BYTES {
            return Err(VerifierError::PublicInputSizeError);
        }
        if hand_commitments.len() > MAX_PLAYERS {
            return Err(VerifierError::WrongCommitmentCount);
        }
        if board_indices.len() != BOARD_INDICES_COUNT {
            return Err(VerifierError::WrongBoardIndicesCount);
        }

        // 1. Verify hand_commitments at field indices 1..7 match stored
        for i in 0..hand_commitments.len() {
            let expected = hand_commitments
                .get(i)
                .ok_or(VerifierError::PublicInputMismatch)?;
            if !Self::check_bytes32_field(&public_inputs, 1 + i, &expected) {
                return Err(VerifierError::PublicInputMismatch);
            }
        }

        // 2. Verify board_indices at field indices 7..12 match on-chain dealt indices
        for i in 0..BOARD_INDICES_COUNT {
            let expected = board_indices
                .get(i)
                .ok_or(VerifierError::PublicInputMismatch)?;
            if !Self::check_u32_field(&public_inputs, 7 + i, expected) {
                return Err(VerifierError::PublicInputMismatch);
            }
        }

        // 3. Verify deck_root at field index 12 matches stored
        if !Self::check_bytes32_field(&public_inputs, 12, &deck_root) {
            return Err(VerifierError::PublicInputMismatch);
        }

        // 4. Run the UltraHonk verification
        Self::verify_proof(env, CircuitType::ShowdownValid, proof, public_inputs)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

    fn setup() -> (Env, ZkVerifierContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ZkVerifierContract, ());
        let client = ZkVerifierContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin)
    }

    #[test]
    fn test_pause_and_unpause() {
        let (_env, client, admin) = setup();
        assert!(!client.is_paused());
        client.pause(&admin);
        assert!(client.is_paused());
        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    #[test]
    fn test_paused_blocks_verify_proof() {
        let (env, client, admin) = setup();
        client.pause(&admin);

        let proof = Bytes::new(&env);
        let public_inputs = Bytes::new(&env);
        let result = client.try_verify_proof(&CircuitType::DealValid, &proof, &public_inputs);
        assert_eq!(result, Err(Ok(VerifierError::ContractPaused)));
    }

    #[test]
    fn test_non_admin_cannot_pause() {
        let (env, client, _admin) = setup();
        let stranger = Address::generate(&env);
        let result = client.try_pause(&stranger);
        assert_eq!(result, Err(Ok(VerifierError::NotAdmin)));
    }

    #[test]
    fn test_admin_can_set_vk_while_paused() {
        let (env, client, admin) = setup();
        client.pause(&admin);
        // set_verification_key is admin-only and must work while paused
        // (VK parsing will fail on empty bytes but should not return ContractPaused)
        let vk = Bytes::new(&env);
        let result = client.try_set_verification_key(&admin, &CircuitType::DealValid, &vk);
        assert!(matches!(result, Err(Ok(VerifierError::VkParseError))));
    }
}
