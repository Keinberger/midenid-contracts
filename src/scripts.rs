use miden_client::{
    Felt, ScriptBuilder, Word,
    account::AccountId,
    crypto::FeltRng,
    note::{NoteAssets, NoteInputs},
    transaction::{OutputNote, TransactionRequestBuilder},
};
use tokio::time::{Duration, sleep};

use crate::{
    accounts::{create_deployer_account, create_naming_account},
    client::{create_keystore, initiate_client},
    notes::{create_note_for_naming, create_pricing_network_note_for_naming},
    transaction::wait_for_tx,
};

pub async fn deploy() -> anyhow::Result<()> {
    println!("Starting Miden Name Registry deployment...");
    println!("=================================================");
    println!("Deleting existing store & keystore (store.sqlite3)");
    let _ = std::fs::remove_file("store.sqlite3");
    let _ = std::fs::remove_dir("keystore");
    println!("Deletion complete.");
    println!("=================================================");

    let mut keystore = create_keystore()?;
    let mut client = initiate_client(keystore.clone()).await?;

    let deployer_account = create_deployer_account(&mut client, &mut keystore).await?;
    let naming_account = create_naming_account(&mut client).await?;
    client.sync_state().await?;

    let price_slot_item_before = naming_account.storage().get_item(2)?;
    println!(
        "Price slot MAST root before: {:?}",
        price_slot_item_before.to_hex()
    );

    println!("=================================================");

    println!("Init note creation started");

    let initialize_inputs = NoteInputs::new(
        [
            Felt::new(deployer_account.id().suffix().into()),
            Felt::new(deployer_account.id().prefix().into()),
            Felt::new(0),
            Felt::new(0),
            Felt::new(5000),
            Felt::new(0),
            Felt::new(0),
            Felt::new(0),
        ]
        .to_vec(),
    )?;
    let init_note = create_note_for_naming(
        "initialize_naming".to_string(),
        initialize_inputs,
        deployer_account.id(),
        naming_account.id(),
        NoteAssets::new(vec![]).unwrap(),
    )
    .await?;

    let init_req = TransactionRequestBuilder::new()
        .own_output_notes(vec![OutputNote::Full(init_note.clone())])
        .build()?;

    let init_tx_id = client
        .submit_new_transaction(deployer_account.id(), init_req)
        .await?;

    println!("Submitted initialization note transaction.");

    println!(
        "View transaction on MidenScan: https://testnet.midenscan.com/tx/{:?}",
        init_tx_id
    );
    client.sync_state().await?;

    println!("naming initialize note creation tx submitted, waiting for onchain commitment");

    wait_for_tx(&mut client, init_tx_id).await?;

    sleep(Duration::from_secs(6)).await;

    client.sync_state().await?;

    println!("=================================================");

    // Consume notes explicitly (required for NoAuth accounts)
    println!("Consuming initialization notes...");

    let consume_request = TransactionRequestBuilder::new()
        .unauthenticated_input_notes([(init_note.clone(), None)])
        .build()?;

    let consume_tx_id = client
        .submit_new_transaction(naming_account.id(), consume_request)
        .await?;
    println!(
        "Consuming initialization notes via transaction: {:?}",
        consume_tx_id
    );

    wait_for_tx(&mut client, consume_tx_id).await?;

    println!("✅ Initialization Notes consumed successfully!");

    println!("=================================================");

    println!("Setting prices");

    println!("Set price note creation started");

    let payment_token_id = AccountId::from_hex("0x54bf4e12ef20082070758b022456c7")?;

    let set_prices_note_inputs = NoteInputs::new(
        [
            Felt::new(payment_token_id.suffix().into()),
            Felt::new(payment_token_id.prefix().into()),
        ]
        .to_vec(),
    )?;

    // let set_prices_note = create_note_for_naming(
    //     "set_all_prices_testnet".to_string(),
    //     set_prices_note_inputs,
    //     deployer_account.id(),
    //     naming_account.id(),
    //     NoteAssets::new(vec![]).unwrap(),
    // )
    // .await?;

    let set_prices_serial_number = client.rng().draw_word();
    let set_prices_note = create_pricing_network_note_for_naming(
        set_prices_serial_number,
        "set_all_prices_testnet".to_string(),
        set_prices_note_inputs,
        deployer_account.id(),
        naming_account.id(),
        NoteAssets::new(vec![]).unwrap(),
    )
    .await?;

    println!("Set prices note ID: {:?}", set_prices_note.id().to_hex());

    let set_price_req = TransactionRequestBuilder::new()
        .own_output_notes(vec![OutputNote::Full(set_prices_note.clone())])
        .build()?;

    let set_prices_tx_id = client
        .submit_new_transaction(deployer_account.id(), set_price_req)
        .await?;

    println!("Submitted set price note transaction.");

    println!(
        "View transaction on MidenScan: https://testnet.midenscan.com/tx/{:?}",
        set_prices_tx_id
    );
    client.sync_state().await?;

    println!("set prices tx submitted, waiting for onchain commitment");

    println!("=================================================");

    wait_for_tx(&mut client, set_prices_tx_id).await?;

    sleep(Duration::from_secs(6)).await;

    client.sync_state().await?;

    sleep(Duration::from_secs(6)).await;

    // println!("Consuming pricing notes...");

    // let consume_request = TransactionRequestBuilder::new()
    //     .unauthenticated_input_notes([(set_prices_note, None)])
    //     .build()?;

    // let consume_tx_id = client
    //     .submit_new_transaction(naming_account.id(), consume_request)
    //     .await?;
    // println!(
    //     "Consuming pricing notes via transaction: {:?}",
    //     consume_tx_id
    // );

    // wait_for_tx(&mut client, consume_tx_id).await?;

    println!("✅ Notes pricing successfully!");

    println!("=================================================");

    println!("✅ Miden Name Registry deployed successfully! ✅");

    println!("Getting price item...");

    let fetched_naming_account = client.get_account(naming_account.id()).await.unwrap();
    let fetched_naming_account_data = fetched_naming_account.unwrap().account().clone();

    // TODO: Use correct price index
    let price_index = Word::new([
        Felt::new(payment_token_id.suffix().into()),
        Felt::new(payment_token_id.prefix().into()),
        Felt::new(1),
        Felt::new(0),
    ]);
    let item = fetched_naming_account_data
        .storage()
        .get_map_item(2, price_index)?;
    println!("Price value: {:?}", item);

    let price_slot_item_after = fetched_naming_account_data.storage().get_item(2)?;
    println!(
        "Price slot MAST root after: {:?}",
        price_slot_item_after.to_hex()
    );

    let init_flag = fetched_naming_account_data.storage().get_item(0)?;
    println!("Init flag: {:?}", init_flag.to_hex());

    Ok(())
}
