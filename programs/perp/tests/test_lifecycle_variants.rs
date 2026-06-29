use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::clock::Clock;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::system_program;
use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use litesvm::LiteSVM;
use litesvm::types::{FailedTransactionMetadata, TransactionMetadata};
use litesvm_token::{
    get_spl_account, spl_token, CreateAssociatedTokenAccount, CreateMint, MintTo, TOKEN_ID,
};
use perp::{
    INSURANCE_FUND_SEED, MARKET_SEED, POSITION_SEED, PRICE_FEED_SEED, VAULT_AUTHORITY_SEED,
    VAULT_SEED,
};
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

// -------- helpers --------

struct TestCtx {
    svm: LiteSVM,
    program_id: Pubkey,
    payer: Keypair,
    usdc_mint: Pubkey,
    price_feed: Pubkey,
    market: Pubkey,
    vault: Pubkey,
    insurance_fund: Pubkey,
    vault_authority: Pubkey,
    user_token_account: Pubkey,
    position: Pubkey,
}

fn setup_with_funded_user(initial_price: u64, mint_amount: u64) -> TestCtx {
    let program_id = perp::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/perp.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    let (price_feed, _) = Pubkey::find_program_address(&[PRICE_FEED_SEED], &program_id);
    let (market, _) = Pubkey::find_program_address(&[MARKET_SEED], &program_id);
    let (vault_authority, _) = Pubkey::find_program_address(&[VAULT_AUTHORITY_SEED], &program_id);
    let (vault, _) = Pubkey::find_program_address(&[VAULT_SEED], &program_id);
    let (insurance_fund, _) = Pubkey::find_program_address(&[INSURANCE_FUND_SEED], &program_id);
    let (position, _) =
        Pubkey::find_program_address(&[POSITION_SEED, payer.pubkey().as_ref()], &program_id);

    let init_feed = Instruction::new_with_bytes(
        program_id,
        &perp::instruction::InitializePriceFeed { initial_price }.data(),
        perp::accounts::InitializePriceFeed {
            payer: payer.pubkey(),
            price_feed,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &payer, &[init_feed]).unwrap();

    let usdc_mint = CreateMint::new(&mut svm, &payer).decimals(6).send().unwrap();

    let init_market = Instruction::new_with_bytes(
        program_id,
        &perp::instruction::InitializeMarket {}.data(),
        perp::accounts::InitializeMarket {
            payer: payer.pubkey(),
            market,
            usdc_mint,
            vault_authority,
            vault,
            insurance_fund,
            oracle: price_feed,
            system_program: system_program::ID,
            token_program: TOKEN_ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &payer, &[init_market]).unwrap();

    let user_token_account =
        CreateAssociatedTokenAccount::new(&mut svm, &payer, &usdc_mint).send().unwrap();
    MintTo::new(&mut svm, &payer, &usdc_mint, &user_token_account, mint_amount)
        .send()
        .unwrap();

    // Seed the vault with USDC liquidity — simulates the LP layer that GMX/Drift have
    // and that Phase 1 doesn't yet model. Without this, any profitable trader payout
    // would fail because the vault has no liquidity beyond traders' own deposits.
    MintTo::new(&mut svm, &payer, &usdc_mint, &vault, 1_000_000_000).send().unwrap();

    TestCtx {
        svm,
        program_id,
        payer,
        usdc_mint,
        price_feed,
        market,
        vault,
        insurance_fund,
        vault_authority,
        user_token_account,
        position,
    }
}

fn send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    ixs: &[Instruction],
) -> Result<TransactionMetadata, FailedTransactionMetadata> {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx)
}

fn open_ix(ctx: &TestCtx, size: i64, collateral_amount: u64) -> Instruction {
    Instruction::new_with_bytes(
        ctx.program_id,
        &perp::instruction::OpenPosition { size, collateral_amount }.data(),
        perp::accounts::OpenPosition {
            user: ctx.payer.pubkey(),
            market: ctx.market,
            position: ctx.position,
            oracle: ctx.price_feed,
            vault: ctx.vault,
            user_token_account: ctx.user_token_account,
            system_program: system_program::ID,
            token_program: TOKEN_ID,
        }
        .to_account_metas(None),
    )
}

fn update_price_ix(ctx: &TestCtx, new_price: u64) -> Instruction {
    Instruction::new_with_bytes(
        ctx.program_id,
        &perp::instruction::UpdatePrice { update_price: new_price }.data(),
        perp::accounts::UpdatePrice {
            authority: ctx.payer.pubkey(),
            price_feed: ctx.price_feed,
        }
        .to_account_metas(None),
    )
}

fn close_ix(ctx: &TestCtx) -> Instruction {
    Instruction::new_with_bytes(
        ctx.program_id,
        &perp::instruction::ClosePosition {}.data(),
        perp::accounts::ClosePosition {
            owner: ctx.payer.pubkey(),
            market: ctx.market,
            position: ctx.position,
            oracle: ctx.price_feed,
            vault: ctx.vault,
            vault_authority: ctx.vault_authority,
            user_token_account: ctx.user_token_account,
            token_program: TOKEN_ID,
        }
        .to_account_metas(None),
    )
}

fn liquidate_ix(
    ctx: &TestCtx,
    liquidator: Pubkey,
    liquidator_token_account: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        ctx.program_id,
        &perp::instruction::LiquidatePosition {}.data(),
        perp::accounts::LiquidatePosition {
            liquidator,
            owner: ctx.payer.pubkey(),
            market: ctx.market,
            position: ctx.position,
            oracle: ctx.price_feed,
            vault: ctx.vault,
            insurance_fund: ctx.insurance_fund,
            vault_authority: ctx.vault_authority,
            liquidator_token_account,
            token_program: TOKEN_ID,
        }
        .to_account_metas(None),
    )
}

/// Mint a fresh liquidator keypair, fund it with SOL for tx fees, and give it an empty
/// USDC token account. The `payer` (main test wallet) pays for the ATA creation so the
/// liquidator starts with exactly 0 USDC — any post-liquidation balance is the reward.
fn create_liquidator(ctx: &mut TestCtx) -> (Keypair, Pubkey) {
    let liquidator = Keypair::new();
    ctx.svm.airdrop(&liquidator.pubkey(), 1_000_000_000).unwrap();
    let liquidator_token_account = CreateAssociatedTokenAccount::new(
        &mut ctx.svm,
        &ctx.payer,
        &ctx.usdc_mint,
    )
    .owner(&liquidator.pubkey())
    .send()
    .unwrap();
    (liquidator, liquidator_token_account)
}

fn token_balance(ctx: &TestCtx, token_account: &Pubkey) -> u64 {
    get_spl_account::<spl_token::state::Account>(&ctx.svm, token_account)
        .unwrap()
        .amount
}

fn sol_balance(ctx: &TestCtx, pubkey: &Pubkey) -> u64 {
    ctx.svm.get_account(pubkey).map(|a| a.lamports).unwrap_or(0)
}

fn crank_ix(ctx: &TestCtx) -> Instruction {
    Instruction::new_with_bytes(
        ctx.program_id,
        &perp::instruction::CrankFunding {}.data(),
        perp::accounts::CrankFunding {
            cranker: ctx.payer.pubkey(),
            market: ctx.market,
            oracle: ctx.price_feed,
        }
        .to_account_metas(None),
    )
}

fn user_balance(ctx: &TestCtx) -> u64 {
    get_spl_account::<spl_token::state::Account>(&ctx.svm, &ctx.user_token_account)
        .unwrap()
        .amount
}

fn read_market(ctx: &TestCtx) -> perp::state::Market {
    let acct = ctx.svm.get_account(&ctx.market).unwrap();
    AccountDeserialize::try_deserialize(&mut &acct.data[..]).unwrap()
}

fn market_oi(ctx: &TestCtx) -> (u64, u64) {
    let m = read_market(ctx);
    (m.open_interest_long, m.open_interest_short)
}

/// Force the Clock sysvar's `unix_timestamp` to a specific value.
/// LiteSVM's clock doesn't auto-advance between txs in lockstep with wall time, so we
/// set it explicitly to make `dt` deterministic for funding-rate assertions.
fn set_clock_ts(svm: &mut LiteSVM, unix_ts: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp = unix_ts;
    svm.set_sysvar(&clock);
}

// -------- tests --------

/// Long position, oracle moves +10%. Expect payout = collateral + (size × Δprice) / BASE_SCALE.
///
/// Starting: 100 USDC. Open 1 SOL long at $100 with 20 USDC collateral.
/// Oracle: $100 → $110. PnL = 1 SOL × $10 = +10 USDC. Payout = 30 USDC.
/// Final user balance: 100 − 20 + 30 = 110 USDC.
#[test]
fn long_with_profit() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, 1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    assert_eq!(user_balance(&ctx), 80_000_000);

    let ix = update_price_ix(&ctx, 110_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let ix = close_ix(&ctx);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    assert_eq!(user_balance(&ctx), 110_000_000);
    assert_eq!(market_oi(&ctx), (0, 0));
    assert!(ctx.svm.get_account(&ctx.position).map_or(true, |a| a.data.is_empty()));
}

/// Long position, oracle moves −5%. PnL is negative but position stays solvent.
///
/// Starting: 100 USDC. Open 1 SOL long at $100 with 20 USDC collateral.
/// Oracle: $100 → $95. PnL = 1 SOL × −$5 = −5 USDC. Payout = 15 USDC.
/// Final user balance: 100 − 20 + 15 = 95 USDC.
#[test]
fn long_with_loss() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, 1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let ix = update_price_ix(&ctx, 95_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let ix = close_ix(&ctx);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    assert_eq!(user_balance(&ctx), 95_000_000);
    assert_eq!(market_oi(&ctx), (0, 0));
}

/// Short position, oracle drops 5%. The signed-size convention should produce positive PnL
/// from the product of two negatives (negative size × negative Δprice).
///
/// Starting: 100 USDC. Open 1 SOL SHORT (size = −1e9) at $100 with 20 USDC collateral.
/// Oracle: $100 → $95. PnL = (−1 SOL) × (−$5) = +5 USDC. Payout = 25 USDC.
/// Final user balance: 100 − 20 + 25 = 105 USDC.
#[test]
fn short_with_profit() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, -1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    // Short OI should track |size| in the short bucket.
    assert_eq!(market_oi(&ctx), (0, 1_000_000_000));

    let ix = update_price_ix(&ctx, 95_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let ix = close_ix(&ctx);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    assert_eq!(user_balance(&ctx), 105_000_000);
    assert_eq!(market_oi(&ctx), (0, 0));
}

/// Underwater long: loss exceeds collateral. Close should revert with `Bankrupt` and
/// leave the position untouched (no payout, OI unchanged, position still on chain).
///
/// Open 1 SOL long at $100 with 20 USDC collateral.
/// Oracle: $100 → $50. PnL = 1 SOL × −$50 = −50 USDC. Signed payout = 20 − 50 = −30 → bankrupt.
#[test]
fn bankrupt_close_reverts() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, 1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    let balance_after_open = user_balance(&ctx);

    let ix = update_price_ix(&ctx, 50_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let ix = close_ix(&ctx);
    let err = send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap_err();
    let err_str = format!("{:?}", err.err);
    // Anchor custom-error code 0x1772 = 6002 = PerpError::Bankrupt.
    assert!(
        err_str.contains("0x1772") || err_str.contains("6002") || err_str.contains("Bankrupt"),
        "expected Bankrupt error, got: {err_str}",
    );

    // Position untouched: user balance, OI, and the Position PDA all unchanged from post-open state.
    assert_eq!(user_balance(&ctx), balance_after_open);
    assert_eq!(market_oi(&ctx), (1_000_000_000, 0));
    let pos_acct = ctx.svm.get_account(&ctx.position).unwrap();
    assert!(!pos_acct.data.is_empty());
}

// -------- funding-crank tests --------

/// Crank with no open positions: cumulative_funding stays 0, but last_funding_ts must still
/// advance to "now". Otherwise the next crank (once positions exist) would integrate over
/// the empty period and produce a giant bogus delta.
#[test]
fn crank_with_no_open_interest_is_noop() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let m_before = read_market(&ctx);
    assert_eq!(m_before.open_interest_long, 0);
    assert_eq!(m_before.open_interest_short, 0);

    let target_ts = m_before.last_funding_ts + 1_000;
    set_clock_ts(&mut ctx.svm, target_ts);

    let ix = crank_ix(&ctx);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let m_after = read_market(&ctx);
    assert_eq!(m_after.cumulative_funding, 0, "no OI → no funding accrual");
    assert_eq!(m_after.last_funding_ts, target_ts, "timestamp must still advance");
}

/// Long-only market: skew is positive, so cumulative_funding accrues positively.
///
/// With DIVISOR = 1e8, dt = 28800s, mark = $100, OI = 1 SOL fully long:
///   delta = skew × mark × dt / (total_oi × DIVISOR)
///         = 1e9 × 1e8 × 28800 / (1e9 × 1e8)
///         = 28800
#[test]
fn crank_long_heavy_market_accrues_positive_funding() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, 1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let m_before = read_market(&ctx);
    let target_ts = m_before.last_funding_ts + 28_800;
    set_clock_ts(&mut ctx.svm, target_ts);

    let ix = crank_ix(&ctx);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let m_after = read_market(&ctx);
    assert_eq!(m_after.cumulative_funding, 28_800);
    assert_eq!(m_after.last_funding_ts, target_ts);
}

/// End-to-end: long pays funding, settlement deducts it from close payout.
///
/// Setup: 100 USDC balance, $100 oracle.
/// Open 1 SOL long with 20 USDC collateral → balance = 80 USDC.
/// Warp +28800s, crank → cumulative_funding = +28800.
/// Close at $100 (no PnL):
///   funding_owed = size × Δcumulative / BASE_SCALE = 1e9 × 28800 / 1e9 = 28800
///   payout = collateral + pnl − funding_owed = 20_000_000 + 0 − 28_800 = 19_971_200
///   final balance = 80_000_000 + 19_971_200 = 99_971_200
#[test]
fn funding_settles_into_long_payout() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, 1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    assert_eq!(user_balance(&ctx), 80_000_000);

    let m_before = read_market(&ctx);
    let target_ts = m_before.last_funding_ts + 28_800;
    set_clock_ts(&mut ctx.svm, target_ts);

    let ix = crank_ix(&ctx);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    assert_eq!(read_market(&ctx).cumulative_funding, 28_800);

    let ix = close_ix(&ctx);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    assert_eq!(user_balance(&ctx), 99_971_200);
    assert_eq!(market_oi(&ctx), (0, 0));
}

/// End-to-end: short-heavy market, short pays funding too (it's the heavy side).
///
/// Setup: 100 USDC balance, $100 oracle.
/// Open 1 SOL SHORT (size = −1e9) with 20 USDC collateral → balance = 80 USDC.
/// Warp +28800s, crank:
///   skew = 0 − 1e9 = −1e9
///   delta = −1e9 × 1e8 × 28800 / (1e9 × 1e8) = −28800
/// Close at $100 (no PnL):
///   funding_owed = size × Δcumulative / BASE_SCALE
///                = (−1e9) × (−28800) / 1e9 = +28800
///   The negative × negative = positive: short owes funding (the heavy side always does).
///   payout = 20_000_000 + 0 − 28_800 = 19_971_200
///   final balance = 99_971_200
#[test]
fn funding_settles_into_short_payout() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, -1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    assert_eq!(user_balance(&ctx), 80_000_000);
    assert_eq!(market_oi(&ctx), (0, 1_000_000_000));

    let m_before = read_market(&ctx);
    let target_ts = m_before.last_funding_ts + 28_800;
    set_clock_ts(&mut ctx.svm, target_ts);

    let ix = crank_ix(&ctx);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    assert_eq!(read_market(&ctx).cumulative_funding, -28_800);

    let ix = close_ix(&ctx);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    assert_eq!(user_balance(&ctx), 99_971_200);
    assert_eq!(market_oi(&ctx), (0, 0));
}

// -------- input-validation tests --------

/// `update_price` with `new_price == 0` must revert. A zero oracle price would zero out
/// every PnL and funding calc, and make all open longs/shorts compute their entry as if
/// the asset were free.
#[test]
fn update_price_rejects_zero() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = update_price_ix(&ctx, 0);
    let err = send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap_err();
    let err_str = format!("{:?}", err.err);
    // Anchor custom-error code 0x1776 = 6006 = PerpError::InvalidPrice.
    assert!(
        err_str.contains("InvalidPrice") || err_str.contains("0x1776") || err_str.contains("6006"),
        "expected InvalidPrice error, got: {err_str}",
    );
}

/// `initialize_price_feed` with `initial_price == 0` must revert. Mirrors the same guarantee
/// as `update_price_rejects_zero`, but at oracle creation time.
#[test]
fn initialize_price_feed_rejects_zero_price() {
    let program_id = perp::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/perp.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    let (price_feed, _) = Pubkey::find_program_address(&[PRICE_FEED_SEED], &program_id);
    let init_feed = Instruction::new_with_bytes(
        program_id,
        &perp::instruction::InitializePriceFeed { initial_price: 0 }.data(),
        perp::accounts::InitializePriceFeed {
            payer: payer.pubkey(),
            price_feed,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let err = send(&mut svm, &payer, &[init_feed]).unwrap_err();
    let err_str = format!("{:?}", err.err);
    // Anchor custom-error code 0x1776 = 6006 = PerpError::InvalidPrice.
    assert!(
        err_str.contains("InvalidPrice") || err_str.contains("0x1776") || err_str.contains("6006"),
        "expected InvalidPrice error, got: {err_str}",
    );
}

/// Back-to-back crank against the same clock: second crank has dt = 0, so it must not
/// change state. Guards against off-by-one timestamp logic in the dt ≤ 0 early-return.
///
/// We have to `expire_blockhash()` between the two cranks because the second tx would
/// otherwise be byte-identical to the first (same payer, same ix data, same blockhash) and
/// the runtime would reject it as a replay before our handler ever ran.
#[test]
fn back_to_back_crank_is_noop() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, 1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let m = read_market(&ctx);
    set_clock_ts(&mut ctx.svm, m.last_funding_ts + 28_800);

    let ix = crank_ix(&ctx);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    let m_after_first = read_market(&ctx);
    assert_eq!(m_after_first.cumulative_funding, 28_800);

    // Force a new blockhash so the second tx's signature differs.
    ctx.svm.expire_blockhash();

    // Same clock state; cranking again should be a no-op (dt = 0).
    let ix = crank_ix(&ctx);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    let m_after_second = read_market(&ctx);
    assert_eq!(m_after_second.cumulative_funding, m_after_first.cumulative_funding);
    assert_eq!(m_after_second.last_funding_ts, m_after_first.last_funding_ts);
}

// -------- liquidation tests --------

/// Underwater long, called by a permissionless liquidator.
///
/// Open 1 SOL long at $100 with 20 USDC collateral. Drop oracle to $84.
/// At $84:
///   equity   = 20_000_000 + (-16_000_000) = 4_000_000
///   notional = 84_000_000
///   threshold = 84_000_000 * 500 / 10_000 = 4_200_000
///   4_000_000 < 4_200_000 → liquidatable.
///
/// Expected:
///   target_reward = 20_000_000 * 100 / 10_000 = 200_000  (1% of collateral)
///   available     = max(equity, 0) = 4_000_000
///   reward        = min(target_reward, available) = 200_000  (= $0.20)
///
/// Position is closed (`close = liquidator` in the accounts struct), so the liquidator
/// receives the position rent in SOL on top of the USDC reward, and OI returns to (0, 0).
#[test]
fn liquidate_underwater_long_succeeds() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, 1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    assert_eq!(market_oi(&ctx), (1_000_000_000, 0));

    let ix = update_price_ix(&ctx, 84_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let (liquidator, liq_ata) = create_liquidator(&mut ctx);
    let liq_sol_before = sol_balance(&ctx, &liquidator.pubkey());
    assert_eq!(token_balance(&ctx, &liq_ata), 0);

    let ix = liquidate_ix(&ctx, liquidator.pubkey(), liq_ata);
    send(&mut ctx.svm, &liquidator, &[ix]).unwrap();

    // USDC reward landed.
    assert_eq!(token_balance(&ctx, &liq_ata), 200_000);
    // Position closed and OI decremented.
    assert_eq!(market_oi(&ctx), (0, 0));
    assert!(ctx.svm.get_account(&ctx.position).map_or(true, |a| a.data.is_empty()));
    // Rent flowed to the liquidator. Liquidator paid one tx fee, but the rent recovered
    // from the closed position dwarfs that, so net SOL must be strictly higher.
    assert!(sol_balance(&ctx, &liquidator.pubkey()) > liq_sol_before);
}

/// Underwater short. Mirror of the long case across the entry price.
///
/// Open 1 SOL SHORT at $100 with 20 USDC collateral. Pump oracle to $115.
/// At $115:
///   equity   = 20_000_000 + (-1e9) * (115e6 - 100e6) / 1e9 = 20_000_000 - 15_000_000 = 5_000_000
///   notional = 1e9 * 115e6 / 1e9 = 115_000_000
///   threshold = 115_000_000 * 500 / 10_000 = 5_750_000
///   5_000_000 < 5_750_000 → liquidatable.
///
///   target_reward = 200_000, available = 5_000_000, reward = 200_000.
#[test]
fn liquidate_underwater_short_succeeds() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, -1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    assert_eq!(market_oi(&ctx), (0, 1_000_000_000));

    let ix = update_price_ix(&ctx, 115_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let (liquidator, liq_ata) = create_liquidator(&mut ctx);

    let ix = liquidate_ix(&ctx, liquidator.pubkey(), liq_ata);
    send(&mut ctx.svm, &liquidator, &[ix]).unwrap();

    assert_eq!(token_balance(&ctx, &liq_ata), 200_000);
    assert_eq!(market_oi(&ctx), (0, 0));
    assert!(ctx.svm.get_account(&ctx.position).map_or(true, |a| a.data.is_empty()));
}

/// A still-healthy long must not be liquidatable. Tests the strict `<` inequality of
/// the threshold check from the safe side: equity sits above MM × notional.
///
/// At $85:
///   equity   = 20_000_000 - 15_000_000 = 5_000_000
///   threshold = 85_000_000 * 500 / 10_000 = 4_250_000
///   5_000_000 < 4_250_000 → false. NotLiquidatable.
///
/// Position must stay untouched: no USDC paid out, no OI change, no account close.
#[test]
fn liquidate_healthy_long_reverts() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, 1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let ix = update_price_ix(&ctx, 85_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let (liquidator, liq_ata) = create_liquidator(&mut ctx);

    let ix = liquidate_ix(&ctx, liquidator.pubkey(), liq_ata);
    let err = send(&mut ctx.svm, &liquidator, &[ix]).unwrap_err();
    let err_str = format!("{:?}", err.err);
    // Anchor custom-error code 0x1778 = 6008 = PerpError::NotLiquidatable.
    assert!(
        err_str.contains("NotLiquidatable")
            || err_str.contains("0x1778")
            || err_str.contains("6008"),
        "expected NotLiquidatable error, got: {err_str}",
    );

    assert_eq!(token_balance(&ctx, &liq_ata), 0);
    assert_eq!(market_oi(&ctx), (1_000_000_000, 0));
    let pos_acct = ctx.svm.get_account(&ctx.position).unwrap();
    assert!(!pos_acct.data.is_empty());
}

/// Symmetric guard on the short side. At $113, short is losing but still healthy.
///
/// At $113:
///   equity   = 20_000_000 - (-1e9 * (113e6 - 100e6) / 1e9) = 20_000_000 - 13_000_000 = 7_000_000
///   threshold = 113_000_000 * 500 / 10_000 = 5_650_000
///   7_000_000 < 5_650_000 → false. NotLiquidatable.
#[test]
fn liquidate_healthy_short_reverts() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, -1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let ix = update_price_ix(&ctx, 113_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let (liquidator, liq_ata) = create_liquidator(&mut ctx);

    let ix = liquidate_ix(&ctx, liquidator.pubkey(), liq_ata);
    let err = send(&mut ctx.svm, &liquidator, &[ix]).unwrap_err();
    let err_str = format!("{:?}", err.err);
    assert!(
        err_str.contains("NotLiquidatable")
            || err_str.contains("0x1778")
            || err_str.contains("6008"),
        "expected NotLiquidatable error, got: {err_str}",
    );

    assert_eq!(token_balance(&ctx, &liq_ata), 0);
    assert_eq!(market_oi(&ctx), (0, 1_000_000_000));
}

/// At entry price, equity = collateral = 20 USDC (way above the $5 threshold). Liquidation
/// must revert. Guards against a regression where the equity formula drops `collateral`
/// (we shipped that bug earlier in this very file before catching it).
#[test]
fn liquidate_unmoved_position_reverts() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, 1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let (liquidator, liq_ata) = create_liquidator(&mut ctx);

    let ix = liquidate_ix(&ctx, liquidator.pubkey(), liq_ata);
    let err = send(&mut ctx.svm, &liquidator, &[ix]).unwrap_err();
    let err_str = format!("{:?}", err.err);
    assert!(
        err_str.contains("NotLiquidatable")
            || err_str.contains("0x1778")
            || err_str.contains("6008"),
        "expected NotLiquidatable error, got: {err_str}",
    );
}

/// Deeply underwater: equity has already gone negative. Liquidation must still succeed
/// (the protocol absolutely wants the position closed), but the USDC reward clamps to 0
/// because `available = max(equity, 0) = 0`. The liquidator's only compensation is the
/// position rent — which is the worst-case incentive design point.
///
/// At $50:
///   equity   = 20_000_000 + (-50_000_000) = -30_000_000  (insolvent)
///   notional = 50_000_000
///   threshold = 2_500_000
///   -30_000_000 < 2_500_000 → liquidatable.
///
///   target_reward = 200_000, available = 0, reward = 0.
#[test]
fn liquidate_bankrupt_long_pays_zero_token_reward() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, 1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let ix = update_price_ix(&ctx, 50_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let (liquidator, liq_ata) = create_liquidator(&mut ctx);
    let liq_sol_before = sol_balance(&ctx, &liquidator.pubkey());

    let ix = liquidate_ix(&ctx, liquidator.pubkey(), liq_ata);
    send(&mut ctx.svm, &liquidator, &[ix]).unwrap();

    // Zero USDC reward — there was nothing positive to pay from.
    assert_eq!(token_balance(&ctx, &liq_ata), 0);
    // Position still closed and OI cleaned up.
    assert_eq!(market_oi(&ctx), (0, 0));
    assert!(ctx.svm.get_account(&ctx.position).map_or(true, |a| a.data.is_empty()));
    // Rent still flows: keepers stay incentivized to clean up bankrupt positions.
    assert!(sol_balance(&ctx, &liquidator.pubkey()) > liq_sol_before);
}

/// Funding integration: a position that's healthy at the current mark price can become
/// liquidatable after enough funding accrues against it. Proves that `funding_owed` is
/// in the equity formula on the liquidation path — not just the close path.
///
/// At mark $86 immediately after open, position is healthy:
///   equity   = 20_000_000 - 14_000_000 = 6_000_000
///   threshold = 86_000_000 * 500 / 10_000 = 4_300_000
///   not liquidatable.
///
/// Advance dt = 2_500_000s, crank. With OI = 1 SOL fully long:
///   delta = 1e9 * 86e6 * 2_500_000 / (1e9 * 1e8) = 2_150_000
///   funding_owed at close = 1e9 * 2_150_000 / 1e9 = 2_150_000
///   new equity = 6_000_000 - 2_150_000 = 3_850_000
///   3_850_000 < 4_300_000 → now liquidatable.
#[test]
fn funding_makes_borderline_position_liquidatable() {
    let mut ctx = setup_with_funded_user(100_000_000, 100_000_000);

    let ix = open_ix(&ctx, 1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let ix = update_price_ix(&ctx, 86_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();

    let (liquidator, liq_ata) = create_liquidator(&mut ctx);

    // Confirm healthy first — equity 6.0 USDC vs threshold 4.3 USDC.
    let ix = liquidate_ix(&ctx, liquidator.pubkey(), liq_ata);
    let err = send(&mut ctx.svm, &liquidator, &[ix]).unwrap_err();
    let err_str = format!("{:?}", err.err);
    assert!(
        err_str.contains("NotLiquidatable")
            || err_str.contains("0x1778")
            || err_str.contains("6008"),
        "expected NotLiquidatable before funding accrual, got: {err_str}",
    );

    // Warp ~29 days and crank. Cumulative funding accumulates a positive delta because
    // skew is fully long, so the long position owes funding.
    let m = read_market(&ctx);
    set_clock_ts(&mut ctx.svm, m.last_funding_ts + 2_500_000);
    ctx.svm.expire_blockhash();
    let ix = crank_ix(&ctx);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    assert_eq!(read_market(&ctx).cumulative_funding, 2_150_000);

    // Same mark, but funding_owed of 2.15 USDC has eroded the cushion. Now liquidatable.
    ctx.svm.expire_blockhash();
    let ix = liquidate_ix(&ctx, liquidator.pubkey(), liq_ata);
    send(&mut ctx.svm, &liquidator, &[ix]).unwrap();

    assert_eq!(market_oi(&ctx), (0, 0));
    assert!(ctx.svm.get_account(&ctx.position).map_or(true, |a| a.data.is_empty()));
    // Reward = min(200_000, max(3_850_000, 0)) = 200_000.
    assert_eq!(token_balance(&ctx, &liq_ata), 200_000);
}
