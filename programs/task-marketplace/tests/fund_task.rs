mod common;

use anchor_lang::prelude::{Clock, Pubkey};
use solana_signer::Signer;
use solana_transaction::InstructionError;
use task_marketplace::{error::ErrorCode, state::TaskStatus};

use common::*;

fn setup_accepted_task() -> (
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_keypair::Keypair,
    Pubkey,
) {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let worker = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    accept_task(&mut svm, &worker, task);
    (svm, creator, worker, task)
}

fn assert_failed_funding_rollback(
    svm: &litesvm::LiteSVM,
    task: Pubkey,
    task_before: &Option<AccountSnapshot>,
    vault: Pubkey,
    vault_before: &Option<AccountSnapshot>,
) {
    assert_account_unchanged(svm, &task, task_before);
    assert_account_unchanged(svm, &vault, vault_before);
    if vault_before.is_none() {
        assert_account_absent(svm, &vault);
    }
    assert_vault_solvent_if_present(svm, &vault);
}

#[test]
fn successful_funding() {
    let (mut svm, creator, worker, task_address) = setup_accepted_task();
    let vault = fund_task(&mut svm, &creator, task_address);

    let task = fetch_task(&svm, &task_address);
    assert!(task.status == TaskStatus::Funded);
    assert_eq!(task.worker, Some(worker.pubkey()));
    assert!(task.funded_at.is_some());
    assert_eq!(
        task.submission_deadline,
        task.funded_at.and_then(
            |timestamp| timestamp.checked_add(task_marketplace::SUBMISSION_TIMEOUT_SECONDS)
        )
    );
    assert_eq!(task.review_deadline, None);
    assert_vault_metadata(&svm, &vault, task_address, DEFAULT_REWARD);
    assert_vault_solvent(&svm, &vault);
}

#[test]
fn non_creator_funding() {
    let (mut svm, _creator, _worker, task) = setup_accepted_task();
    let attacker = funded_keypair(&mut svm);
    let vault = vault_pda(&task).0;
    let task_before = snapshot_account(&svm, &task);
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &attacker,
        fund_task_instruction(attacker.pubkey(), task, vault),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::Unauthorized);
    assert_failed_funding_rollback(&svm, task, &task_before, vault, &vault_before);
}

#[test]
fn funding_open_task() {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    let vault = vault_pda(&task).0;
    let task_before = snapshot_account(&svm, &task);
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &creator,
        fund_task_instruction(creator.pubkey(), task, vault),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::InvalidStateTransition);
    assert_failed_funding_rollback(&svm, task, &task_before, vault, &vault_before);
}

#[test]
fn funding_task_without_worker() {
    let (mut svm, creator, _worker, task_address) = setup_accepted_task();
    let mut task = fetch_task(&svm, &task_address);
    task.worker = None;
    overwrite_task(&mut svm, task_address, &task);
    let vault = vault_pda(&task_address).0;
    let task_before = snapshot_account(&svm, &task_address);
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &creator,
        fund_task_instruction(creator.pubkey(), task_address, vault),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::InvalidStateTransition);
    assert_failed_funding_rollback(&svm, task_address, &task_before, vault, &vault_before);
}

#[test]
fn replay_funding() {
    let (mut svm, creator, _worker, task) = setup_accepted_task();
    let vault = fund_task(&mut svm, &creator, task);
    let task_before = snapshot_account(&svm, &task);
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &creator,
        fund_task_instruction(creator.pubkey(), task, vault),
        &[],
    )
    .unwrap_err();

    assert_custom_error(&error, 0);
    assert_failed_funding_rollback(&svm, task, &task_before, vault, &vault_before);
    assert_eq!(fetch_vault(&svm, &vault).escrowed_lamports, DEFAULT_REWARD);
}

#[test]
fn wrong_vault_pda() {
    let (mut svm, creator, _worker, task) = setup_accepted_task();
    let correct_vault = vault_pda(&task).0;
    let wrong_vault = Pubkey::new_unique();
    let task_before = snapshot_account(&svm, &task);
    let correct_vault_before = snapshot_account(&svm, &correct_vault);
    let wrong_vault_before = snapshot_account(&svm, &wrong_vault);

    let error = send_instruction(
        &mut svm,
        &creator,
        fund_task_instruction(creator.pubkey(), task, wrong_vault),
        &[],
    )
    .unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_unchanged(&svm, &task, &task_before);
    assert_account_unchanged(&svm, &correct_vault, &correct_vault_before);
    assert_account_unchanged(&svm, &wrong_vault, &wrong_vault_before);
    assert_account_absent(&svm, &correct_vault);
    assert_account_absent(&svm, &wrong_vault);
    assert_vault_solvent_if_present(&svm, &correct_vault);
}

#[test]
fn insufficient_balance() {
    let (mut svm, creator, _worker, task) = setup_accepted_task();
    let vault = vault_pda(&task).0;
    let vault_rent = svm.minimum_balance_for_rent_exemption(8 + 106);
    set_balance(
        &mut svm,
        creator.pubkey(),
        vault_rent + DEFAULT_REWARD + 5_000 - 1,
    );
    let task_before = snapshot_account(&svm, &task);
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &creator,
        fund_task_instruction(creator.pubkey(), task, vault),
        &[],
    )
    .unwrap_err();

    assert_eq!(
        error.err,
        solana_transaction::TransactionError::InstructionError(0, InstructionError::Custom(1)),
        "logs:\n{}",
        error.meta.pretty_logs()
    );
    assert_failed_funding_rollback(&svm, task, &task_before, vault, &vault_before);
}

#[test]
fn vault_metadata_correctness() {
    let (mut svm, creator, _worker, task) = setup_accepted_task();
    let vault = fund_task(&mut svm, &creator, task);

    assert_vault_metadata(&svm, &vault, task, DEFAULT_REWARD);
    assert_eq!(svm.get_account(&vault).unwrap().owner, task_marketplace::ID);
    assert_eq!(svm.get_account(&vault).unwrap().data.len(), 8 + 106);
    assert_vault_solvent(&svm, &vault);
}

#[test]
fn escrow_accounting_invariant() {
    let (mut svm, creator, _worker, task) = setup_accepted_task();
    let vault = fund_task(&mut svm, &creator, task);
    let account = svm.get_account(&vault).unwrap();
    let rent = svm.minimum_balance_for_rent_exemption(account.data.len());
    let escrowed = fetch_vault(&svm, &vault).escrowed_lamports;

    assert_eq!(escrowed, fetch_task(&svm, &task).reward_amount);
    assert!(account.lamports >= rent.checked_add(escrowed).unwrap());
    assert_vault_solvent(&svm, &vault);
}

#[test]
fn donation_after_funding() {
    let (mut svm, creator, _worker, task) = setup_accepted_task();
    let vault = fund_task(&mut svm, &creator, task);
    let donor = funded_keypair(&mut svm);
    let vault_before = snapshot_account(&svm, &vault).unwrap();
    let donation = 123_456;

    transfer_lamports(&mut svm, &donor, vault, donation);

    let vault_after = snapshot_account(&svm, &vault).unwrap();
    assert_eq!(vault_after.lamports, vault_before.lamports + donation);
    assert_eq!(vault_after.data, vault_before.data);
    assert_eq!(fetch_vault(&svm, &vault).escrowed_lamports, DEFAULT_REWARD);
    assert_vault_solvent(&svm, &vault);
}

#[test]
fn pre_funded_vault_pda() {
    let (mut svm, creator, _worker, task) = setup_accepted_task();
    let vault = vault_pda(&task).0;
    let donor = funded_keypair(&mut svm);
    let donation = 10_000;
    transfer_lamports(&mut svm, &donor, vault, donation);
    let pre_funded_balance = balance(&svm, &vault);
    let rent = svm.minimum_balance_for_rent_exemption(8 + 106);

    fund_task(&mut svm, &creator, task);

    assert_eq!(
        balance(&svm, &vault),
        pre_funded_balance.max(rent) + DEFAULT_REWARD
    );
    assert_vault_metadata(&svm, &vault, task, DEFAULT_REWARD);
    assert_vault_solvent(&svm, &vault);
}

#[test]
fn atomic_rollback_on_transfer_failure() {
    let (mut svm, creator, _worker, task) = setup_accepted_task();
    let vault = vault_pda(&task).0;
    let vault_rent = svm.minimum_balance_for_rent_exemption(8 + 106);
    set_balance(
        &mut svm,
        creator.pubkey(),
        vault_rent + DEFAULT_REWARD + 5_000 - 1,
    );
    let task_before = snapshot_account(&svm, &task);
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &creator,
        fund_task_instruction(creator.pubkey(), task, vault),
        &[],
    )
    .unwrap_err();

    assert_eq!(
        error.err,
        solana_transaction::TransactionError::InstructionError(0, InstructionError::Custom(1)),
        "logs:\n{}",
        error.meta.pretty_logs()
    );
    assert_failed_funding_rollback(&svm, task, &task_before, vault, &vault_before);
    let task = fetch_task(&svm, &task);
    assert!(task.status == TaskStatus::Accepted);
    assert_eq!(task.funded_at, None);
}

#[test]
fn timestamp_correctness() {
    let (mut svm, creator, _worker, task) = setup_accepted_task();
    let expected_timestamp = 1_735_689_600;
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp = expected_timestamp;
    svm.set_sysvar(&clock);

    let vault = fund_task(&mut svm, &creator, task);

    let funded_task = fetch_task(&svm, &task);
    assert_eq!(funded_task.funded_at, Some(expected_timestamp));
    assert_eq!(
        funded_task.submission_deadline,
        Some(expected_timestamp + task_marketplace::SUBMISSION_TIMEOUT_SECONDS)
    );
    assert_eq!(funded_task.review_deadline, None);
    assert_vault_solvent(&svm, &vault);
}
