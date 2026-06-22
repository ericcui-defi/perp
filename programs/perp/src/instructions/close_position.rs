use anchor_lang::prelude::*;
use crate::{BASE_SCALE, MARKET_SEED, POSITION_SEED, VAULT_AUTHORITY_SEED, state::{Market, Position, PriceFeed}};
use anchor_spl::token::{Token, TokenAccount};
use anchor_spl::token::{self, Transfer};
use crate::error::PerpError;

#[derive(Accounts)]
pub struct ClosePosition<'info> {

    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(mut, seeds = [MARKET_SEED], bump = market.bump)]
    pub market: Account<'info, Market>,

    #[account(
        mut,
        close = owner,
        has_one = owner,
        seeds = [POSITION_SEED, owner.key().as_ref()],
        bump = position.bump,
    )]
    pub position: Account<'info, Position>,

    #[account(address = market.oracle)]
    pub oracle: Account<'info, PriceFeed>,

    #[account(mut, address = market.vault)]
    pub vault: Account<'info, TokenAccount>,

    /// CHECK: Phantom signing PDA. No data; identity verified by seeds + bump.
    #[account(seeds = [VAULT_AUTHORITY_SEED], bump)]
    pub vault_authority: UncheckedAccount<'info>,

    #[account(mut, token::mint = vault.mint, token::authority = owner)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<ClosePosition>) -> Result<()> {

    // PnL calculation
    let size = ctx.accounts.position.size;
    let entry_price = ctx.accounts.position.entry_price;
    let price = ctx.accounts.oracle.price;
    let funding_snapshot = ctx.accounts.position.funding_snapshot;
    let cumulative_funding = ctx.accounts.market.cumulative_funding;
    let collateral = ctx.accounts.position.collateral;

    let pnl = (size as i128 * (price as i128 - entry_price as i128)) / BASE_SCALE as i128;

    let funding_owed = (size as i128 * (cumulative_funding as i128 - funding_snapshot as i128)) / BASE_SCALE as i128;

    let payout_signed = collateral as i128 + pnl - funding_owed;
    require!(payout_signed >= 0, PerpError::Bankrupt);
    // Bound-check before casting. Without this, a wildly profitable position could compute
    // a payout above u64::MAX and silently truncate to a wrong value via `as u64`.
    let payout = u64::try_from(payout_signed).map_err(|_| PerpError::PayoutOverflow)?;

    // Token CPI: vault -> user, signed by vault_authority PDA.
    let vault_auth_bump = ctx.bumps.vault_authority;
    let signer_seeds: &[&[&[u8]]] = &[&[VAULT_AUTHORITY_SEED, &[vault_auth_bump]]];
    let cpi = CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.vault_authority.to_account_info(),
        },
        signer_seeds,
    );
    token::transfer(cpi, payout)?;

    let market = &mut ctx.accounts.market;
    if size > 0 {
        market.open_interest_long = market.open_interest_long.checked_sub(size as u64).unwrap();
  } else {
        market.open_interest_short = market.open_interest_short.checked_sub(size.unsigned_abs()).unwrap();
  }
    Ok(())
}

