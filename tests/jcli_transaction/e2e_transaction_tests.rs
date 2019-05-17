#![allow(dead_code)]

use common::configuration::genesis_model::Fund;
use common::jcli_wrapper;
use common::jcli_wrapper::jcli_transaction_wrapper::JCLITransactionWrapper;
use common::startup;

#[test]
#[cfg(feature = "integration-test")]
pub fn test_utxo_transation_with_more_than_one_witness_per_input_is_rejected() {
    let sender = startup::create_new_utxo_address();
    let reciever = startup::create_new_utxo_address();
    let mut config = startup::from_initial_funds(vec![Fund {
        address: sender.address.clone(),
        value: 100,
    }]);

    let jormungandr_rest_address = config.get_node_address();
    let _jormungandr = startup::start_jormungandr_node_as_leader(&mut config);
    let utxo = startup::get_utxo_for_address(&sender, &jormungandr_rest_address);
    let block0_hash = jcli_wrapper::assert_genesis_hash(&config.genesis_block_path);

    let mut transaction_wrapper = JCLITransactionWrapper::new_transaction(&block0_hash);
    transaction_wrapper
        .assert_add_input_from_utxo(&utxo)
        .assert_add_output(&reciever.address, &utxo.out_value)
        .assert_finalize();

    let witness1 = transaction_wrapper.create_witness_default("utxo");
    let witness2 = transaction_wrapper.create_witness_default("utxo");

    transaction_wrapper
        .assert_make_witness(&witness1)
        .assert_add_witness(&witness1)
        .assert_make_witness(&witness2)
        .assert_add_witness_fail(&witness2, "too many witnesses");
}

#[test]
#[cfg(feature = "integration-test")]
pub fn test_two_correct_utxo_to_utxo_transactions_are_accepted_by_node() {
    let sender = startup::create_new_utxo_address();
    let middle_man = startup::create_new_utxo_address();
    let reciever = startup::create_new_utxo_address();
    let mut config = startup::from_initial_funds(vec![Fund {
        address: sender.address.clone(),
        value: 100,
    }]);

    let jormungandr_rest_address = config.get_node_address();
    let _jormungandr = startup::start_jormungandr_node_as_leader(&mut config);

    let utxo = startup::get_utxo_for_address(&sender, &jormungandr_rest_address);
    let block0_hash = jcli_wrapper::assert_genesis_hash(&config.genesis_block_path);
    let transaction_builder = JCLITransactionWrapper::build_transaction_from_utxo(
        &utxo,
        &utxo.out_value,
        &middle_man,
        &utxo.out_value,
        &sender,
        &block0_hash,
    );

    let transaction_message = transaction_builder.assert_transaction_to_message();
    let first_transaction_id = transaction_builder.get_transaction_id();

    jcli_wrapper::assert_transaction_post_accepted(&transaction_message, &jormungandr_rest_address);

    let transaction_message = JCLITransactionWrapper::build_transaction(
        &first_transaction_id,
        &0,
        &100,
        &reciever,
        &100,
        &middle_man,
        &block0_hash,
    )
    .assert_transaction_to_message();

    jcli_wrapper::assert_transaction_post_accepted(&transaction_message, &jormungandr_rest_address);
}

#[test]
#[cfg(feature = "integration-test")]
pub fn test_correct_utxo_transaction_is_accepted_by_node() {
    let sender = startup::create_new_utxo_address();
    let reciever = startup::create_new_utxo_address();

    let mut config = startup::from_initial_funds(vec![Fund {
        address: sender.address.clone(),
        value: 100,
    }]);

    let jormungandr_rest_address = config.get_node_address();
    let _jormungandr = startup::start_jormungandr_node_as_leader(&mut config);
    let block0_hash = jcli_wrapper::assert_genesis_hash(&config.genesis_block_path);
    let utxo = startup::get_utxo_for_address(&sender, &jormungandr_rest_address);

    let transaction_message = JCLITransactionWrapper::new_transaction(&block0_hash)
        .assert_add_input_from_utxo(&utxo)
        .assert_add_output(&reciever.address, &utxo.out_value)
        .assert_finalize()
        .seal_with_witness_deafult(&sender.private_key, "utxo")
        .assert_transaction_to_message();

    jcli_wrapper::assert_transaction_post_accepted(&transaction_message, &jormungandr_rest_address);
}

#[test]
#[cfg(feature = "integration-test")]
pub fn test_transaction_from_utxo_to_account_is_accepted_by_node() {
    let sender = startup::create_new_utxo_address();
    let reciever = startup::create_new_account_address();

    let mut config = startup::from_initial_funds(vec![Fund {
        address: sender.address.clone(),
        value: 100,
    }]);

    let jormungandr_rest_address = config.get_node_address();
    let _jormungandr = startup::start_jormungandr_node_as_leader(&mut config);
    let block0_hash = jcli_wrapper::assert_genesis_hash(&config.genesis_block_path);
    let utxo = startup::get_utxo_for_address(&sender, &jormungandr_rest_address);

    let transaction_message = JCLITransactionWrapper::new_transaction(&block0_hash)
        .assert_add_input_from_utxo(&utxo)
        .assert_add_output(&reciever.address, &utxo.out_value)
        .assert_finalize()
        .seal_with_witness_deafult(&sender.private_key, "utxo")
        .assert_transaction_to_message();

    jcli_wrapper::assert_transaction_post_accepted(&transaction_message, &jormungandr_rest_address);
}

#[test]
#[cfg(feature = "integration-test")]
pub fn test_transaction_from_account_to_account_is_accepted_by_node() {
    let sender = startup::create_new_account_address();
    let reciever = startup::create_new_account_address();
    let transfer_amount = 100;
    let mut config = startup::from_initial_funds(vec![Fund {
        address: sender.address.clone(),
        value: transfer_amount.clone(),
    }]);

    let jormungandr_rest_address = config.get_node_address();
    let _jormungandr = startup::start_jormungandr_node_as_leader(&mut config);
    let block0_hash = jcli_wrapper::assert_genesis_hash(&config.genesis_block_path);

    let transaction_message = JCLITransactionWrapper::new_transaction(&block0_hash)
        .assert_add_account(&sender.address, &transfer_amount)
        .assert_add_output(&reciever.address, &transfer_amount)
        .assert_finalize()
        .seal_with_witness_deafult(&sender.private_key, "account")
        .assert_transaction_to_message();

    jcli_wrapper::assert_transaction_post_accepted(&transaction_message, &jormungandr_rest_address);
}
