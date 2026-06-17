
use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::system_program;
use anchor_lang::{InstructionData, ToAccountMetas};
use litesvm::LiteSVM;
use litesvm_token::{CreateMint, TOKEN_ID};
use perp::{MARKET_SEED, PRICE_FEED_SEED, VAULT_AUTHORITY_SEED, VAULT_SEED};
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

#[test]
fn test_initialize() {
    let program_id = perp::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/perp.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();


    let (price_feed, _) = Pubkey::find_program_address(
        &[PRICE_FEED_SEED],
        &program_id
    );
    let price_feed_instruction = Instruction::new_with_bytes(
        program_id,
        &perp::instruction::InitializePriceFeed {
            initial_price: 100_000_000
        }.data(),
        perp::accounts::InitializePriceFeed {
            payer: payer.pubkey(),
            price_feed: price_feed,
            system_program: system_program::ID,
        }.to_account_metas(None),
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[price_feed_instruction], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer]).unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_ok());


    // Deriving necessary PDAs
    let (market, _) = Pubkey::find_program_address(
        &[MARKET_SEED], 
        &program_id);
    let (vault_authority, _) = Pubkey:: find_program_address(
        &[VAULT_AUTHORITY_SEED],
        &program_id);
    let (vault, _) = Pubkey::find_program_address(
        &[VAULT_SEED],
        &program_id
    );

    // Create the USDC mint via litesvm-token's CreateMint builder.
    // Internally: allocates the mint keypair, runs create_account + initialize_mint2,
    // signs with payer + the internal mint keypair, returns the mint pubkey.
    let usdc_mint = CreateMint::new(&mut svm, &payer)
        .decimals(6)
        .send()
        .unwrap();
    
    let instruction = Instruction::new_with_bytes(
        program_id,
        &perp::instruction::InitializeMarket {}.data(),
        perp::accounts::InitializeMarket {
            payer: payer.pubkey(),
            market: market,
            usdc_mint: usdc_mint,
            vault_authority: vault_authority,
            vault: vault,
            oracle: price_feed,
            system_program: system_program::ID,
            token_program: TOKEN_ID,
        }.to_account_metas(None),
    );


    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer]).unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_ok());

    let acct = svm.get_account(&market).unwrap();
    let m: perp::state::Market = anchor_lang::AccountDeserialize::try_deserialize(&mut &acct.data[..]).unwrap();
    assert_eq!(m.oracle, price_feed);
    assert_eq!(m.vault, vault);
    assert_eq!(m.cumulative_funding, 0);
}
