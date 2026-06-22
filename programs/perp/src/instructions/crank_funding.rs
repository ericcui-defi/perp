use anchor_lang::prelude::*;
use crate::state::{Market, PriceFeed};
use crate::constants::*;

#[derive(Accounts)]
pub struct CrankFunding<'info> {

    pub cranker: Signer<'info>,

    #[account(mut, seeds = [MARKET_SEED], bump = market.bump)]
    pub market: Account<'info, Market>,

    #[account(address = market.oracle)]
    pub oracle: Account<'info, PriceFeed>,

}

pub fn handler(ctx: Context<CrankFunding>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let mark = ctx.accounts.oracle.price as i128;
    let market = &mut ctx.accounts.market;

    let dt = now - market.last_funding_ts;
    if dt <= 0 {
        return Ok(());
    }

    let total_oi = (market.open_interest_long as i128) + (market.open_interest_short as i128);
    if total_oi == 0 {
        market.last_funding_ts = now;
        return Ok(());
    }

    let skew = (market.open_interest_long as i128) - (market.open_interest_short as i128);
    let delta = (skew * mark * dt as i128) / (total_oi * FUNDING_RATE_DIVISOR as i128);

    market.cumulative_funding = market.cumulative_funding.checked_add(delta).unwrap();
    market.last_funding_ts = now;
    Ok(())
}


