pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("XpJ2J8tahL9bf5Y6L4DCdj5oPBvtTAmo6uF8REuvrkm");

#[program]
pub mod perp {
    use super::*;

    pub fn initialize_market(ctx: Context<InitializeMarket>) -> Result<()> {
        initialize_market::handler(ctx)
    }
    pub fn initialize_price_feed(ctx: Context<InitializePriceFeed>, initial_price: u64) -> Result<()> {
        initialize_price_feed::handler(ctx, initial_price)
    }
    pub fn update_price(ctx: Context<UpdatePrice>, update_price: u64) -> Result<()> {
        update_price::handler(ctx, update_price)
    }
    pub fn open_position(ctx: Context<OpenPosition>, size: i64, collateral_amount: u64) -> Result<()> {
        open_position::handler(ctx, size, collateral_amount)
    }
    pub fn close_position(ctx: Context<ClosePosition>) -> Result<()> {
        close_position::handler(ctx)
    }
    pub fn crank_funding(ctx: Context<CrankFunding>) -> Result<()> {
        crank_funding::handler(ctx)
    }
    pub fn liquidate_position(ctx: Context<LiquidatePosition>) -> Result<()> {
        liquidate_position::handler(ctx)
    }
    pub fn add_collateral(ctx: Context<AddCollateral>, amount: u64) -> Result<()> {
        add_collateral::handler(ctx, amount)
    }
}
