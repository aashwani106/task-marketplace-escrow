mod common;

use anchor_lang::prelude::Pubkey;
use solana_signer::Signer;
use task_marketplace::{
    error::ErrorCode,
    state::{EscrowVault, TaskStatus},
};

use common::*;

const SUBMISSION_REFERENCE: &str = "ipfs://pay-task-integration-test";

fn setup_submitted_task() -> (
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_keypair::Keypair,
    Pubkey,
    Pubkey,
) {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let worker = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    accept_task(&mut svm, &worker, task);
    let vault = fund_task(&mut svm, &creator, task);
    submit_task(&mut svm, &worker, task, SUBMISSION_REFERENCE);
    (svm, creator, worker, task, vault)
}

fn assert_failed_payment_rollback(
    svm: &litesvm::LiteSVM,
    task: Pubkey,
    task_before: &Option<AccountSnapshot>,
    vault: Pubkey,
    vault_before: &Option<AccountSnapshot>,
    worker: Pubkey,
    worker_balance_before: u64,
) {
    assert_account_unchanged(svm, &task, task_before);
    assert_account_unchanged(svm, &vault, vault_before);
    assert_eq!(balance(svm, &worker), worker_balance_before);
}

#[test]
fn successful_payment() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let worker_before = balance(&svm, &worker.pubkey());
    let worker_amount = fetch_vault(&svm, &vault).escrowed_lamports;

    pay_task(&mut svm, &creator, task, vault, worker.pubkey());

    assert!(fetch_task(&svm, &task).status == TaskStatus::Paid);
    assert_eq!(
        balance(&svm, &worker.pubkey()) - worker_before,
        worker_amount
    );
    assert_account_absent(&svm, &vault);
}

#[test]
fn creator_receives_rent_reserve() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let creator_before = balance(&svm, &creator.pubkey());
    let vault_before = balance(&svm, &vault);
    let worker_amount = fetch_vault(&svm, &vault).escrowed_lamports;
    let expected_creator_refund = vault_before - worker_amount;

    let metadata = pay_task(&mut svm, &creator, task, vault, worker.pubkey());

    assert_eq!(
        balance(&svm, &creator.pubkey()) + metadata.fee - creator_before,
        expected_creator_refund
    );
    assert_eq!(
        expected_creator_refund,
        svm.minimum_balance_for_rent_exemption(8 + 106)
    );
    assert_account_absent(&svm, &vault);
}

#[test]
fn creator_receives_donation_surplus() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let donor = funded_keypair(&mut svm);
    let donation = 123_456;
    transfer_lamports(&mut svm, &donor, vault, donation);
    let creator_before = balance(&svm, &creator.pubkey());
    let vault_before = balance(&svm, &vault);
    let worker_amount = fetch_vault(&svm, &vault).escrowed_lamports;

    let metadata = pay_task(&mut svm, &creator, task, vault, worker.pubkey());

    assert_eq!(
        balance(&svm, &creator.pubkey()) + metadata.fee - creator_before,
        vault_before - worker_amount
    );
    assert_eq!(
        vault_before - worker_amount,
        svm.minimum_balance_for_rent_exemption(8 + 106) + donation
    );
    assert_account_absent(&svm, &vault);
}

#[test]
fn worker_receives_exactly_escrowed_amount() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let worker_before = balance(&svm, &worker.pubkey());
    let worker_amount = fetch_vault(&svm, &vault).escrowed_lamports;

    pay_task(&mut svm, &creator, task, vault, worker.pubkey());

    assert_eq!(
        balance(&svm, &worker.pubkey()),
        worker_before + worker_amount
    );
}

#[test]
fn vault_is_closed_after_payment() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();

    pay_task(&mut svm, &creator, task, vault, worker.pubkey());

    assert_account_absent(&svm, &vault);
    assert!(fetch_task(&svm, &task).status == TaskStatus::Paid);
}

#[test]
fn wrong_creator() {
    let (mut svm, _creator, worker, task, vault) = setup_submitted_task();
    let attacker = funded_keypair(&mut svm);
    assert_failing_payment(&mut svm, &attacker, task, vault, worker.pubkey(), |error| {
        assert_task_marketplace_error(error, ErrorCode::Unauthorized)
    });
}

#[test]
fn wrong_worker_account() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let wrong_worker = funded_keypair(&mut svm);
    let stored_worker_before = balance(&svm, &worker.pubkey());

    assert_failing_payment(
        &mut svm,
        &creator,
        task,
        vault,
        wrong_worker.pubkey(),
        |error| assert_task_marketplace_error(error, ErrorCode::Unauthorized),
    );
    assert_eq!(balance(&svm, &worker.pubkey()), stored_worker_before);
}

#[test]
fn wrong_vault_pda() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let wrong_vault = Pubkey::new_unique();
    svm.set_account(wrong_vault, svm.get_account(&vault).unwrap())
        .unwrap();
    let task_before = snapshot_account(&svm, &task);
    let vault_before = snapshot_account(&svm, &vault);
    let wrong_vault_before = snapshot_account(&svm, &wrong_vault);
    let worker_before = balance(&svm, &worker.pubkey());

    let error = send_instruction(
        &mut svm,
        &creator,
        pay_task_instruction(creator.pubkey(), task, wrong_vault, worker.pubkey()),
        &[],
    )
    .unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_unchanged(&svm, &task, &task_before);
    assert_account_unchanged(&svm, &vault, &vault_before);
    assert_account_unchanged(&svm, &wrong_vault, &wrong_vault_before);
    assert_eq!(balance(&svm, &worker.pubkey()), worker_before);
    assert_vault_solvent(&svm, &vault);
}

#[test]
fn replay_payment() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    pay_task(&mut svm, &creator, task, vault, worker.pubkey());
    let task_before = snapshot_account(&svm, &task);
    let worker_before = balance(&svm, &worker.pubkey());

    let error = send_instruction(
        &mut svm,
        &creator,
        pay_task_instruction(creator.pubkey(), task, vault, worker.pubkey()),
        &[],
    )
    .unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::AccountNotInitialized);
    assert_account_unchanged(&svm, &task, &task_before);
    assert_account_absent(&svm, &vault);
    assert_eq!(balance(&svm, &worker.pubkey()), worker_before);
}

#[test]
fn open_task_cannot_be_paid() {
    assert_status_cannot_be_paid(TaskStatus::Open);
}

#[test]
fn accepted_task_cannot_be_paid() {
    assert_status_cannot_be_paid(TaskStatus::Accepted);
}

#[test]
fn funded_task_cannot_be_paid() {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let worker = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    accept_task(&mut svm, &worker, task);
    let vault = fund_task(&mut svm, &creator, task);

    assert_failing_payment(&mut svm, &creator, task, vault, worker.pubkey(), |error| {
        assert_task_marketplace_error(error, ErrorCode::InvalidStateTransition)
    });
}

#[test]
fn corrupted_vault_version() {
    let (mut svm, creator, worker, task, vault_address) = setup_submitted_task();
    let mut vault: EscrowVault = fetch_vault(&svm, &vault_address);
    vault.version = vault.version.checked_add(1).unwrap();
    overwrite_vault(&mut svm, vault_address, &vault);

    assert_failing_payment(
        &mut svm,
        &creator,
        task,
        vault_address,
        worker.pubkey(),
        |error| assert_task_marketplace_error(error, ErrorCode::InvalidVaultVersion),
    );
}

#[test]
fn corrupted_vault_task_key() {
    let (mut svm, creator, worker, task, vault_address) = setup_submitted_task();
    let mut vault: EscrowVault = fetch_vault(&svm, &vault_address);
    vault.task = Pubkey::new_unique();
    overwrite_vault(&mut svm, vault_address, &vault);

    assert_failing_payment(
        &mut svm,
        &creator,
        task,
        vault_address,
        worker.pubkey(),
        |error| assert_task_marketplace_error(error, ErrorCode::InvalidVaultTask),
    );
}

#[test]
fn corrupted_vault_liability() {
    let (mut svm, creator, worker, task, vault_address) = setup_submitted_task();
    let mut vault: EscrowVault = fetch_vault(&svm, &vault_address);
    vault.escrowed_lamports = vault.escrowed_lamports.checked_add(1).unwrap();
    overwrite_vault(&mut svm, vault_address, &vault);

    assert_failing_payment(
        &mut svm,
        &creator,
        task,
        vault_address,
        worker.pubkey(),
        |error| assert_task_marketplace_error(error, ErrorCode::InvalidEscrowLiability),
    );
}

#[test]
fn accounting_solvency_validation() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let escrowed = fetch_vault(&svm, &vault).escrowed_lamports;
    let rent = svm.minimum_balance_for_rent_exemption(8 + 106);
    set_balance(&mut svm, vault, rent + escrowed - 1);

    assert_failing_payment(&mut svm, &creator, task, vault, worker.pubkey(), |error| {
        assert_task_marketplace_error(error, ErrorCode::EscrowBalanceMismatch)
    });
}

fn assert_status_cannot_be_paid(status: TaskStatus) {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let mut task_account = fetch_task(&svm, &task);
    task_account.status = status;
    overwrite_task(&mut svm, task, &task_account);

    assert_failing_payment(&mut svm, &creator, task, vault, worker.pubkey(), |error| {
        assert_task_marketplace_error(error, ErrorCode::InvalidStateTransition)
    });
}

fn assert_failing_payment(
    svm: &mut litesvm::LiteSVM,
    creator: &solana_keypair::Keypair,
    task: Pubkey,
    vault: Pubkey,
    worker: Pubkey,
    assert_error: impl FnOnce(&litesvm::types::FailedTransactionMetadata),
) {
    let task_before = snapshot_account(svm, &task);
    let vault_before = snapshot_account(svm, &vault);
    let worker_before = balance(svm, &worker);

    let error = send_instruction(
        svm,
        creator,
        pay_task_instruction(creator.pubkey(), task, vault, worker),
        &[],
    )
    .unwrap_err();

    assert_error(&error);
    assert_failed_payment_rollback(
        svm,
        task,
        &task_before,
        vault,
        &vault_before,
        worker,
        worker_before,
    );
}
