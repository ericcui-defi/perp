use anchor_lang::prelude::*;
use crate::state::{Market, Position, PriceFeed};
use crate::constants::*;
use anchor_spl::token::{TokenAccount, Token, Transfer, self};
use crate::error::PerpError;

#[derive(Accounts)]
pub struct LiquidatePosition<'info> {

    #[account(mut)]
    pub liquidator: Signer<'info>,

    
    /// CHECK: Position owner. Not a signer. Needed only as the seed input
    /// for the position PDA and as the rent destination if you want rent
    /// returned to them (see note below).
    pub owner: UncheckedAccount<'info>,

    #[account(mut, seeds = [MARKET_SEED], bump = market.bump)]
    pub market: Account<'info, Market>,

    #[account(mut, close = liquidator, seeds = [POSITION_SEED, owner.key().as_ref()], bump = position.bump)]
    pub position: Account<'info, Position>,

    #[account(address = market.oracle)]
    pub oracle: Account<'info, PriceFeed>,

    #[account(mut, address = market.vault)]
    pub vault: Account<'info, TokenAccount>,

    /// CHECK: Phantom signing PDA. No data; identity verified by seeds + bump.
    #[account(seeds = [VAULT_AUTHORITY_SEED], bump)]
    pub vault_authority: UncheckedAccount<'info>,

    #[account(mut, token::mint = vault.mint, token::authority = liquidator)]
    pub liquidator_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<LiquidatePosition>) -> Result<()> {

    // Check whether this is a liquidatable position
    let mark = ctx.accounts.oracle.price;
    let entry_price = ctx.accounts.position.entry_price;
    let funding_snapshot = ctx.accounts.position.funding_snapshot;
    let cumulative_funding = ctx.accounts.market.cumulative_funding;
    let size = ctx.accounts.position.size;
    let collateral = ctx.accounts.position.collateral;

    let funding_owed = (size as i128 * (cumulative_funding - funding_snapshot)) / BASE_SCALE as i128;

    // Equity — works for long and short because `size` is signed.
    let margin = collateral as i128
        + (size as i128 * (mark as i128 - entry_price as i128)) / BASE_SCALE as i128
        - funding_owed;

    let notional = (size.unsigned_abs() as i128 * mark as i128) / BASE_SCALE as i128;
    let threshold = (notional * MAINTENANCE_MARGIN_BPS as i128) / 10_000;

    // Liquidation instruction wrongly initiated
    require!(margin < threshold, PerpError::NotLiquidatable);

    // Calculate liquidation reward
    // Reward is 1% of the trader's collateral, but capped at remaining positive equity
    // so a deeply-bankrupt position can't pay out more than is actually in the vault for it.
    let target_reward = (collateral as i128 * LIQUIDATION_REWARD_BPS as i128) / 10_000;
    let available = margin.max(0);
    let liquidation_reward = target_reward.min(available) as u64;

    // Transfer liquidation reward to liquidator ATA
    let vault_auth_bump = ctx.bumps.vault_authority;
    let signer_seeds: &[&[&[u8]]] = &[&[VAULT_AUTHORITY_SEED, &[vault_auth_bump]]];
    let cpi = CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        Transfer{
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.liquidator_token_account.to_account_info(),
            authority: ctx.accounts.vault_authority.to_account_info(),
        },
        signer_seeds,
    );
    token::transfer(cpi, liquidation_reward)?;

    let market = &mut ctx.accounts.market;
    if size > 0 {
        market.open_interest_long = market.open_interest_long.checked_sub(size as u64).unwrap();
    } else {
        market.open_interest_short = market.open_interest_short.checked_sub(size.unsigned_abs()).unwrap();
    }

    Ok(())
}


