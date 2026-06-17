use anchor_lang::prelude::*;

#[constant]
pub const PRICE_SCALE: u64 = 1_000_000; // USDC native

#[constant]
pub const BASE_SCALE: u64 = 1_000_000_000; // SOL lamports

#[constant]
pub const MARKET_SEED: &[u8] = b"market";

#[constant]
pub const POSITION_SEED: &[u8] = b"position";

#[constant]
pub const VAULT_AUTHORITY_SEED: &[u8] = b"vault_authority";

#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

#[constant]
pub const PRICE_FEED_SEED: &[u8] = b"price_feed";
