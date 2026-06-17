use anchor_lang::prelude::*;
use crate::{MARKET_SEED, POSITION_SEED, state::{Market, Position, PriceFeed}};
use anchor_spl::token::{Token, TokenAccount};
use anchor_spl::token::{self, Transfer};
use crate::error::PerpError;

#[derive(Accounts)]
pub struct OpenPosition<'info> {
    
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(mut, seeds = [MARKET_SEED], bump = market.bump)]
    pub market: Account<'info, Market>,

    #[account(init, payer = user, space = 8 + Position::INIT_SPACE, seeds = [POSITION_SEED, user.key().as_ref()], bump)]
    pub position: Account<'info, Position>,

    #[account(address = market.oracle)]
    pub oracle: Account<'info, PriceFeed>,

    #[account(mut, address = market.vault)]
    pub vault: Account<'info, TokenAccount>,
    
    #[account(mut, token::mint = vault.mint, token::authority = user)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<OpenPosition>, size: i64, collateral_amount: u64) -> Result<()> {
    
    // Validation
    require!(size != 0, PerpError::ZeroPositionSize);
    require!(collateral_amount > 0, PerpError::ZeroCollateral);

    let cpi = CpiContext::new(
        ctx.accounts.token_program.key(),
        Transfer{
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        }
    );
    token::transfer(cpi, collateral_amount)?;

    let position = &mut ctx.accounts.position;
    position.bump = ctx.bumps.position;
    position.owner = ctx.accounts.user.key();
    position.collateral = collateral_amount;
    position.size = size;
    position.entry_price = ctx.accounts.oracle.price;
    position.funding_snapshot = ctx.accounts.market.cumulative_funding;

    let market = &mut ctx.accounts.market;
    if size > 0 {
        market.open_interest_long = market.open_interest_long.checked_add(size as u64).unwrap();
    } else {
        market.open_interest_short = market.open_interest_short.checked_add(size.unsigned_abs()).unwrap();
    }

    Ok(())
}