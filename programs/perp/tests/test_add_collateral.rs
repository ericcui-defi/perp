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

fn add_collateral_ix(ctx: &TestCtx, amount: u64) -> Instruction {
    Instruction::new_with_bytes(
        ctx.program_id,
        &perp::instruction::AddCollateral { amount }.data(),
        perp::accounts::AddCollateral {
            owner: ctx.payer.pubkey(),
            market: ctx.market,
            position: ctx.position,
            vault: ctx.vault,
            user_token_account: ctx.user_token_account,
            token_program: TOKEN_ID,
        }
        .to_account_metas(None),
    )
}

fn token_balance(ctx: &TestCtx, token_account: &Pubkey) -> u64 {
    get_spl_account::<spl_token::state::Account>(&ctx.svm, token_account)
        .unwrap()
        .amount
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

fn read_position(ctx: &TestCtx) -> perp::state::Position {
    let acct = ctx.svm.get_account(&ctx.position).unwrap();
    AccountDeserialize::try_deserialize(&mut &acct.data[..]).unwrap()
}

#[test]
fn test_add_collateral() {
    let mut ctx = setup_with_funded_user(100_000_000, 2_000_000_000);

    let ix = open_ix(&ctx, 1_000_000_000, 20_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    assert_eq!(user_balance(&ctx), 1_980_000_000);
    assert_eq!(read_position(&ctx).collateral, 20_000_000);

    let ix = add_collateral_ix(&ctx, 1_000_000_000);
    send(&mut ctx.svm, &ctx.payer, &[ix]).unwrap();
    assert_eq!(user_balance(&ctx), 980_000_000);
    assert_eq!(read_position(&ctx).collateral, 1_020_000_000);
    assert_eq!(token_balance(&ctx, &read_market(&ctx).vault), 2_020_000_000);
}
