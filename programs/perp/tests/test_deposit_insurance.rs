use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::system_program;
use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use litesvm::LiteSVM;
use litesvm::types::{FailedTransactionMetadata, TransactionMetadata};
use litesvm_token::{
    get_spl_account, spl_token, CreateAssociatedTokenAccount, CreateMint, MintTo, TOKEN_ID,
};
use perp::{INSURANCE_FUND_SEED, MARKET_SEED, PRICE_FEED_SEED, VAULT_AUTHORITY_SEED, VAULT_SEED};
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
    market: Pubkey,
    insurance_fund: Pubkey,
    payer_token_account: Pubkey,
}

fn setup_with_funded_user(mint_amount: u64) -> TestCtx {
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

    let init_feed = Instruction::new_with_bytes(
        program_id,
        &perp::instruction::InitializePriceFeed { initial_price: 100_000_000 }.data(),
        perp::accounts::InitializePriceFeed {
            payer: payer.pubkey(),
            price_feed,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &payer, &payer, &[init_feed]).unwrap();

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
    send(&mut svm, &payer, &payer, &[init_market]).unwrap();

    let payer_token_account =
        CreateAssociatedTokenAccount::new(&mut svm, &payer, &usdc_mint).send().unwrap();
    MintTo::new(&mut svm, &payer, &usdc_mint, &payer_token_account, mint_amount)
        .send()
        .unwrap();

    TestCtx {
        svm,
        program_id,
        payer,
        usdc_mint,
        market,
        insurance_fund,
        payer_token_account,
    }
}

fn send(
    svm: &mut LiteSVM,
    fee_payer: &Keypair,
    signer: &Keypair,
    ixs: &[Instruction],
) -> Result<TransactionMetadata, FailedTransactionMetadata> {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&fee_payer.pubkey()), &blockhash);
    // If fee_payer == signer, dedup to one signer.
    let tx = if fee_payer.pubkey() == signer.pubkey() {
        VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[fee_payer]).unwrap()
    } else {
        VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[fee_payer, signer]).unwrap()
    };
    svm.send_transaction(tx)
}

fn deposit_insurance_ix(
    ctx: &TestCtx,
    user: &Pubkey,
    user_token_account: &Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        ctx.program_id,
        &perp::instruction::DepositInsurance { amount }.data(),
        perp::accounts::DepositInsurance {
            user: *user,
            market: ctx.market,
            insurance_fund: ctx.insurance_fund,
            user_token_account: *user_token_account,
            token_program: TOKEN_ID,
        }
        .to_account_metas(None),
    )
}

fn token_balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    get_spl_account::<spl_token::state::Account>(svm, token_account).unwrap().amount
}

fn read_market(ctx: &TestCtx) -> perp::state::Market {
    let acct = ctx.svm.get_account(&ctx.market).unwrap();
    AccountDeserialize::try_deserialize(&mut &acct.data[..]).unwrap()
}

/// Creates a new keypair, airdrops SOL, opens an ATA, mints `mint_amount` USDC to it.
fn create_funded_user(ctx: &mut TestCtx, mint_amount: u64) -> (Keypair, Pubkey) {
    let user = Keypair::new();
    ctx.svm.airdrop(&user.pubkey(), 1_000_000_000).unwrap();
    let ata = CreateAssociatedTokenAccount::new(&mut ctx.svm, &ctx.payer, &ctx.usdc_mint)
        .owner(&user.pubkey())
        .send()
        .unwrap();
    MintTo::new(&mut ctx.svm, &ctx.payer, &ctx.usdc_mint, &ata, mint_amount)
        .send()
        .unwrap();
    (user, ata)
}

// -------- tests --------

/// Happy path: depositor signs a deposit, USDC flows from their ATA into the
/// insurance fund. The instruction's whole job in one assertion pair.
#[test]
fn deposit_insurance_happy_path() {
    let mut ctx = setup_with_funded_user(100_000_000);

    // Sanity: insurance fund starts empty.
    assert_eq!(token_balance(&ctx.svm, &ctx.insurance_fund), 0);

    let ix = deposit_insurance_ix(&ctx, &ctx.payer.pubkey(), &ctx.payer_token_account, 30_000_000);
    send(&mut ctx.svm, &ctx.payer, &ctx.payer, &[ix]).unwrap();

    assert_eq!(token_balance(&ctx.svm, &ctx.insurance_fund), 30_000_000);
    assert_eq!(token_balance(&ctx.svm, &ctx.payer_token_account), 70_000_000);

    // Market state is unchanged — deposit_insurance is purely a token transfer.
    let m = read_market(&ctx);
    assert_eq!(m.open_interest_long, 0);
    assert_eq!(m.open_interest_short, 0);
    assert_eq!(m.cumulative_funding, 0);
}

/// Two deposits accumulate in the same pool — the fund is additive, not
/// overwriting. Catches "transfer one but record the other" mistakes.
#[test]
fn deposit_insurance_accumulates() {
    let mut ctx = setup_with_funded_user(100_000_000);

    let ix1 = deposit_insurance_ix(&ctx, &ctx.payer.pubkey(), &ctx.payer_token_account, 10_000_000);
    send(&mut ctx.svm, &ctx.payer, &ctx.payer, &[ix1]).unwrap();

    let ix2 = deposit_insurance_ix(&ctx, &ctx.payer.pubkey(), &ctx.payer_token_account, 25_000_000);
    send(&mut ctx.svm, &ctx.payer, &ctx.payer, &[ix2]).unwrap();

    assert_eq!(token_balance(&ctx.svm, &ctx.insurance_fund), 35_000_000);
    assert_eq!(token_balance(&ctx.svm, &ctx.payer_token_account), 65_000_000);
}

/// A user with no special role (not the market authority, not the payer)
/// can deposit. Confirms the permissionless design property — anyone can
/// donate to the insurance fund.
#[test]
fn deposit_insurance_permissionless() {
    let mut ctx = setup_with_funded_user(0);
    let (donor, donor_ata) = create_funded_user(&mut ctx, 50_000_000);

    // donor is NOT ctx.payer (the market authority). They sign their own tx.
    let ix = deposit_insurance_ix(&ctx, &donor.pubkey(), &donor_ata, 50_000_000);
    send(&mut ctx.svm, &donor, &donor, &[ix]).unwrap();

    assert_eq!(token_balance(&ctx.svm, &ctx.insurance_fund), 50_000_000);
    assert_eq!(token_balance(&ctx.svm, &donor_ata), 0);
}

/// A user trying to deposit using someone else's token account fails the
/// `token::authority = user` constraint. Guards against the case where a
/// malicious signer points the `from` account at a victim's ATA.
#[test]
fn deposit_insurance_wrong_authority_reverts() {
    let mut ctx = setup_with_funded_user(100_000_000);
    let (attacker, _) = create_funded_user(&mut ctx, 0);

    // Attacker signs, but `from` ATA is the payer's. Anchor's
    // `token::authority = user` constraint must reject.
    let ix =
        deposit_insurance_ix(&ctx, &attacker.pubkey(), &ctx.payer_token_account, 1_000_000);
    let res = send(&mut ctx.svm, &attacker, &attacker, &[ix]);
    assert!(res.is_err(), "deposit with mismatched authority should revert");

    // Fund untouched.
    assert_eq!(token_balance(&ctx.svm, &ctx.insurance_fund), 0);
    assert_eq!(token_balance(&ctx.svm, &ctx.payer_token_account), 100_000_000);
}
