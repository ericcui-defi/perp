use anchor_lang::prelude::*;
use crate::{MARKET_SEED, POSITION_SEED, state::{Market, Position}};
use anchor_spl::token::{Token, TokenAccount};
use anchor_spl::token::{self, Transfer};

#[derive(Accounts)]
pub struct AddCollateral<'info> {

    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(seeds = [MARKET_SEED], bump = market.bump)]
    pub market: Account<'info, Market>,

    #[account(mut, seeds = [POSITION_SEED, owner.key().as_ref()], bump = position.bump)]
    pub position: Account<'info, Position>,

    #[account(mut, address = market.vault)]
    pub vault: Account<'info, TokenAccount>,

    #[account(mut, token::mint = vault.mint, token::authority = owner)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,

}

pub fn handler(ctx: Context<AddCollateral>, amount: u64) -> Result<()> {

    // Transfer token
    let cpi = CpiContext::new(
        ctx.accounts.token_program.key(),
        Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
            authority: ctx.accounts.owner.to_account_info(),
        }
    );
    token::transfer(cpi, amount)?;

    // Update position
    ctx.accounts.position.collateral = ctx.accounts.position.collateral.checked_add(amount).unwrap();

    Ok(())
}