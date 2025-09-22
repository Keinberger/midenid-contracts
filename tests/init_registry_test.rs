use midenid_contracts::common::{
    create_basic_account, create_library, create_public_immutable_contract,
    create_public_note_with_library, create_tx_script, delete_keystore_and_store,
    instantiate_client, wait_for_note,
};

use miden_client::{
    ClientError, Felt, Word,
    account::AddressInterface,
    account::{
        AccountBuilder, AccountIdAddress, AccountStorageMode, AccountType, Address, StorageSlot,
    },
    keystore::FilesystemKeyStore,
    note::NoteAssets,
    rpc::Endpoint,
    transaction::TransactionRequestBuilder,
};
use miden_lib::account::auth::{self};
use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    account::{AccountComponent, NetworkId},
    assembly::Assembler,
};
use rand::RngCore;
use std::{fs, path::Path};
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn init_registry_with_note() -> Result<(), ClientError> {
    delete_keystore_and_store().await;

    let endpoint = Endpoint::testnet();
    let mut client = instantiate_client(endpoint.clone()).await.unwrap();

    let keystore = FilesystemKeyStore::new("./keystore".into()).unwrap();

    let sync_summary = client.sync_state().await.unwrap();
    println!("Latest block: {}", sync_summary.block_num);

    // -------------------------------------------------------------------------
    // STEP 1: Create Basic User Account
    // -------------------------------------------------------------------------
    let (alice_account, _) = create_basic_account(&mut client, keystore.clone())
        .await
        .unwrap();

    let alice_account_address =
        AccountIdAddress::new(alice_account.id(), AddressInterface::BasicWallet);

    // build address of faucet
    let alice_account_address = Address::AccountId(alice_account_address);

    println!(
        "alice account id: {:?}",
        alice_account_address.to_bech32(NetworkId::Testnet)
    );

    // -------------------------------------------------------------------------
    // STEP 2: Create Counter Smart Contract
    // -------------------------------------------------------------------------
    let counter_code = fs::read_to_string(Path::new("./masm/accounts/miden_id.masm")).unwrap();

    let assembler: Assembler = TransactionKernel::assembler().with_debug_mode(true);

    let counter_component = AccountComponent::compile(
        counter_code.clone(),
        assembler.clone(),
        vec![StorageSlot::Value(Word::new([
            Felt::new(0),
            Felt::new(0),
            Felt::new(0),
            Felt::new(0),
        ]))],
    )
    .unwrap()
    .with_supports_all_types();

    let mut init_seed = [0_u8; 32];
    client.rng().fill_bytes(&mut init_seed);
    let (counter_contract, counter_seed) = AccountBuilder::new(init_seed)
        .account_type(AccountType::RegularAccountImmutableCode)
        .storage_mode(AccountStorageMode::Public)
        .with_auth_component(auth::NoAuth)
        .with_component(counter_component.clone())
        .build()
        .unwrap();

    let counter_contract_address =
        AccountIdAddress::new(counter_contract.id(), AddressInterface::Unspecified);

    // build address of faucet
    let counter_contract_address = Address::AccountId(counter_contract_address);
    println!(
        "contract id: {:?}",
        counter_contract_address.to_bech32(NetworkId::Testnet)
    );

    client
        .add_account(&counter_contract, Some(counter_seed), false)
        .await
        .unwrap();

    // -------------------------------------------------------------------------
    // STEP 3: Prepare & Create the Note
    // -------------------------------------------------------------------------
    let note_code = fs::read_to_string(Path::new("./masm/notes/init_miden_id.masm")).unwrap();
    let account_code = fs::read_to_string(Path::new("./masm/accounts/miden_id.masm")).unwrap();

    let library_path = "miden_id::registry";
    let library = create_library(account_code, library_path).unwrap();

    let note_assets = NoteAssets::new(vec![]).unwrap();

    let increment_note = create_public_note_with_library(
        &mut client,
        note_code,
        alice_account.clone(),
        note_assets,
        library,
    )
    .await
    .unwrap();

    println!("Init note created, waiting for onchain commitment");

    // Give time for transaction to be processed before looking for the note
    // This prevents the wait_for_note function from getting stuck
    sleep(Duration::from_secs(3)).await;

    // -------------------------------------------------------------------------
    // STEP 4: Consume the Note
    // -------------------------------------------------------------------------
    // wait_for_note(&mut client, Some(counter_contract.clone()), &increment_note)
    //     .await
    //     .unwrap();

    let consume_custom_req = TransactionRequestBuilder::new()
        .unauthenticated_input_notes([(increment_note.clone(), None)])
        .build()
        .unwrap();

    let tx_result = client
        .new_transaction(counter_contract.id(), consume_custom_req)
        .await
        .unwrap();

    let submission_result = client.submit_transaction(tx_result).await;
    if let Err(e) = submission_result {
        eprintln!("Failed to submit consumption transaction: {}", e);
        panic!("Transaction submission failed: {}", e);
    }
    println!("Note consumption transaction submitted successfully");

    // -------------------------------------------------------------------------
    // STEP 5: Validate Updated State
    // -------------------------------------------------------------------------
    sleep(Duration::from_secs(5)).await;

    delete_keystore_and_store().await;

    let mut client = instantiate_client(endpoint).await.unwrap();

    client
        .import_account_by_id(counter_contract.id())
        .await
        .unwrap();

    let new_account_state = client.get_account(counter_contract.id()).await.unwrap();

    if let Some(account) = new_account_state.as_ref() {
        let count: Word = account.account().storage().get_item(0).unwrap().into();
        let val = count.get(3).unwrap().as_int();
        assert_eq!(val, 1);
    }

    Ok(())
}
