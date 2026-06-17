use anchor_lang::prelude::*;
use crate::constants::*;
use crate::state::PriceFeed;

#[derive(Accounts)]
pub struct UpdatePrice<'info> {

    pub authority: Signer<'info>, 

    #[account(mut, has_one = authority, seeds = [PRICE_FEED_SEED], bump = price_feed.bump)]
    pub price_feed: Account<'info, PriceFeed>,
}

pub fn handler(ctx: Context<UpdatePrice>, update_price: u64) -> Result<()> {
    let price_feed = &mut ctx.accounts.price_feed;
    price_feed.price = update_price;
    price_feed.last_updated_ts = Clock::get()?.unix_timestamp;
    Ok(())
}