
use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::system_program;
use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use litesvm::LiteSVM;
use litesvm_token::{CreateAssociatedTokenAccount, CreateMint, MintTo, TOKEN_ID, get_spl_account, spl_token};
use perp::{INSURANCE_FUND_SEED, MARKET_SEED, POSITION_SEED, PRICE_FEED_SEED, VAULT_AUTHORITY_SEED, VAULT_SEED};
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

#[test]
fn test_open() {
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
    let (insurance_fund, _) = Pubkey::find_program_address(
        &[INSURANCE_FUND_SEED],
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
            insurance_fund: insurance_fund,
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

    // Deriving position PDA
    let (position, _) = Pubkey::find_program_address(
        &[POSITION_SEED, payer.pubkey().as_ref()],
        &program_id
    );

    let user_token_account = CreateAssociatedTokenAccount::new(&mut svm, &payer, &usdc_mint)
        .send()
        .unwrap();

    MintTo::new(&mut svm, &payer, &usdc_mint, &user_token_account, 50_000_000)
        .send()
        .unwrap();

    // Open position instruction
    let instruction = Instruction::new_with_bytes(
        program_id,
        &perp::instruction::OpenPosition { size: 1_000_000_000, collateral_amount: 20_000_000}.data(),
        perp::accounts::OpenPosition {
                user: payer.pubkey(),
                market: market,
                position: position,
                oracle: price_feed,
                vault: vault,
                user_token_account: user_token_account,
                system_program: system_program::ID,
                token_program: TOKEN_ID,
        }.to_account_metas(None),
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer]).unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_ok());

    let market_acct = svm.get_account(&market).unwrap();
    let m: perp::state::Market = anchor_lang::AccountDeserialize::try_deserialize(
        &mut &market_acct.data[..]
    ).unwrap();
    assert_eq!(m.open_interest_long, 1_000_000_000);
    assert_eq!(m.open_interest_short, 0);

    let position_acct = svm.get_account(&position).unwrap();
    let p: perp::state::Position = anchor_lang::AccountDeserialize::try_deserialize(
        &mut &position_acct.data[..]
    ).unwrap();
    assert_eq!(p.collateral, 20_000_000);
    assert_eq!(p.size, 1_000_000_000);
    assert_eq!(p.entry_price, 100_000_000);
    assert_eq!(p.funding_snapshot, 0);

}

#[test]
fn test_close() {

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
    let (insurance_fund, _) = Pubkey::find_program_address(
        &[INSURANCE_FUND_SEED],
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
            insurance_fund: insurance_fund,
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

    // Deriving position PDA
    let (position, _) = Pubkey::find_program_address(
        &[POSITION_SEED, payer.pubkey().as_ref()],
        &program_id
    );

    let user_token_account = CreateAssociatedTokenAccount::new(&mut svm, &payer, &usdc_mint)
        .send()
        .unwrap();

    MintTo::new(&mut svm, &payer, &usdc_mint, &user_token_account, 50_000_000)
        .send()
        .unwrap();

    // Open position instruction
    let instruction = Instruction::new_with_bytes(
        program_id,
        &perp::instruction::OpenPosition { size: 1_000_000_000, collateral_amount: 20_000_000}.data(),
        perp::accounts::OpenPosition {
                user: payer.pubkey(),
                market: market,
                position: position,
                oracle: price_feed,
                vault: vault,
                user_token_account: user_token_account,
                system_program: system_program::ID,
                token_program: TOKEN_ID,
        }.to_account_metas(None),
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer]).unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_ok());

    let market_acct = svm.get_account(&market).unwrap();
    let m: perp::state::Market = anchor_lang::AccountDeserialize::try_deserialize(
        &mut &market_acct.data[..]
    ).unwrap();
    assert_eq!(m.open_interest_long, 1_000_000_000);
    assert_eq!(m.open_interest_short, 0);

    let position_acct = svm.get_account(&position).unwrap();
    let p: perp::state::Position = anchor_lang::AccountDeserialize::try_deserialize(
        &mut &position_acct.data[..]
    ).unwrap();
    assert_eq!(p.collateral, 20_000_000);
    assert_eq!(p.size, 1_000_000_000);
    assert_eq!(p.entry_price, 100_000_000);
    assert_eq!(p.funding_snapshot, 0);

    // Close account instruction
    let instruction = Instruction::new_with_bytes(
        program_id,
        &perp::instruction::ClosePosition {}.data(),
        perp::accounts::ClosePosition {
            owner: payer.pubkey(),
            market: market,
            position: position,
            oracle: price_feed,
            vault: vault,
            vault_authority: vault_authority,
            user_token_account: user_token_account,
            token_program: TOKEN_ID,
        }.to_account_metas(None),
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer]).unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_ok());

    let market_acct = svm.get_account(&market).unwrap();
    let m: perp::state::Market = AccountDeserialize::try_deserialize(&mut &market_acct.data[..]).unwrap();
    assert_eq!(m.open_interest_long, 0);

    // Position no longer exists (or has empty data)
    let pos_acct = svm.get_account(&position);
    assert!(pos_acct.is_none() || pos_acct.unwrap().data.is_empty());

    // User got their collateral back (no PnL, same oracle price)
    let bal = get_spl_account::<spl_token::state::Account>(&svm, &user_token_account).unwrap().amount;
    assert_eq!(bal, 50_000_000);   // started with 50, deposited 20, got 20 back at close
}
