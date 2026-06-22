use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Market {
    pub bump: u8,
    pub authority: Pubkey,
    pub oracle: Pubkey,
    pub vault: Pubkey,
    pub cumulative_funding: i128,
    pub open_interest_long: u64,
    pub open_interest_short: u64,
    pub last_funding_ts: i64,
}

#[account]
#[derive(InitSpace)]
pub struct Position {
    pub bump: u8,
    pub owner: Pubkey,
    pub collateral: u64,
    pub size: i64,
    pub entry_price: u64,
    pub funding_snapshot: i128,
}

#[account]
#[derive(InitSpace)]
pub struct PriceFeed {
    pub bump: u8,
    pub authority: Pubkey,
    pub price: u64,
    pub last_updated_ts: i64,
}