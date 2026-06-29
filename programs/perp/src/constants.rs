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

#[constant]
pub const FUNDING_RATE_DIVISOR: i64 = 100_000_000;

#[constant]
pub const MAINTENANCE_MARGIN_BPS: u64 = 500;

#[constant]
pub const LIQUIDATION_REWARD_BPS: u64 = 100;

#[constant]
pub const INSURANCE_FUND_SEED: &[u8] = b"insurance_fund";
