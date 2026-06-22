use anchor_lang::prelude::*;
use crate::state::PriceFeed;
use crate::constants::*;
use crate::error::PerpError;

#[derive(Accounts)]
pub struct InitializePriceFeed<'info> {

    // Payer that pays to put the oracle on-chain
    #[account(mut)]
    pub payer: Signer<'info>,

    // Initializing data-containing price feed account
    #[account(init, payer = payer, space = 8 + PriceFeed::INIT_SPACE, seeds = [PRICE_FEED_SEED], bump)]
    pub price_feed: Account<'info, PriceFeed>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<InitializePriceFeed>, initial_price: u64) -> Result<()> {
    require!(initial_price > 0, PerpError::InvalidPrice);

    let price_feed = &mut ctx.accounts.price_feed;
    price_feed.authority = ctx.accounts.payer.key();
    price_feed.bump = ctx.bumps.price_feed;
    price_feed.price = initial_price;
    price_feed.last_updated_ts = Clock::get()?.unix_timestamp;
    Ok(())
}