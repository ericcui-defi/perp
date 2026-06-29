use anchor_lang::prelude::*;
use crate::{MARKET_SEED, state::Market};
use anchor_spl::token::{Token, TokenAccount};
use anchor_spl::token::{self, Transfer};

#[derive(Accounts)]
pub struct DepositInsurance<'info> {

    #[account(mut)]
    pub user: Signer<'info>,

    #[account(seeds = [MARKET_SEED], bump = market.bump)]
    pub market: Account<'info, Market>,

    #[account(mut, address = market.insurance_fund)]
    pub insurance_fund: Account<'info, TokenAccount>,


    #[account(mut, token::mint = insurance_fund.mint, token::authority = user)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<DepositInsurance>, amount: u64) -> Result<()> {

    // Transfer token
    let cpi = CpiContext::new(
        ctx.accounts.token_program.key(),
        Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.insurance_fund.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        }
    );
    token::transfer(cpi, amount)?;

    Ok(())
}

