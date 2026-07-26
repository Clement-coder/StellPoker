use soroban_sdk::{xdr::ToXdr, Address, BytesN, Env};

pub fn bytes32_eq(left: &BytesN<32>, right: &BytesN<32>) -> bool {
    let left_arr = left.to_array();
    let right_arr = right.to_array();
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= left_arr[i] ^ right_arr[i];
    }
    diff == 0
}

pub fn u32_eq(left: u32, right: u32) -> bool {
    (left ^ right) == 0
}

pub fn u32_ne(left: u32, right: u32) -> bool {
    !u32_eq(left, right)
}

pub fn i128_eq(left: i128, right: i128) -> bool {
    ((left ^ right) as u128) == 0
}

pub fn address_eq(env: &Env, left: &Address, right: &Address) -> bool {
    let left_hash: BytesN<32> = env.crypto().keccak256(&left.to_xdr(env)).into();
    let right_hash: BytesN<32> = env.crypto().keccak256(&right.to_xdr(env)).into();
    bytes32_eq(&left_hash, &right_hash)
}

pub fn address_ne(env: &Env, left: &Address, right: &Address) -> bool {
    !address_eq(env, left, right)
}
