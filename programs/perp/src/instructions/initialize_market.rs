use anchor_lang::prelude::*;
use crate::state::Market;
use anchor_spl::token::{Mint, Token, TokenAccount};
use crate::constants::*;

#[derive(Accounts)]
pub struct InitializeMarket<'info> {

    // Payer
    #[account(mut)]
    pub payer: Signer<'info>,

    // Market PDA
    #[account(init, payer = payer, space = 8 + Market::INIT_SPACE, seeds = [MARKET_SEED], bump)]
    pub market: Account<'info, Market>,

    pub usdc_mint: Account<'info, Mint>,

    /// CHECK: Phantom signing PDA. No data; identify verified by seeds + bump
    #[account(seeds = [VAULT_AUTHORITY_SEED], bump)]
    pub vault_authority: UncheckedAccount<'info>,

    #[account(init, payer = payer, token::mint = usdc_mint, token::authority = vault_authority, seeds = [VAULT_SEED],bump)]
    pub vault: Account<'info, TokenAccount>,

    /// CHECK: Pyth feed address; captured into Market for future validation
    pub oracle: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<InitializeMarket>) -> Result<()> {
    
    let market =  &mut ctx.accounts.market;
    market.bump = ctx.bumps.market;
    market.authority = ctx.accounts.payer.key();
    market.oracle = ctx.accounts.oracle.key();
    market.vault = ctx.accounts.vault.key();
    market.cumulative_funding = 0;
    market.open_interest_long = 0;
    market.open_interest_short = 0;
    Ok(())
}
