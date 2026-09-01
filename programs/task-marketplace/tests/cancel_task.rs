mod common;

use anchor_lang::prelude::Pubkey;
use solana_signer::Signer;
use task_marketplace::{error::ErrorCode, state::TaskStatus};

use common::*;

const SUBMISSION_REFERENCE: &str = "ipfs://cancel-task-integration-test";

fn setup_open_task() -> (litesvm::LiteSVM, solana_keypair::Keypair, Pubkey) {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    (svm, creator, task)
}

fn setup_accepted_task() -> (
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_keypair::Keypair,
    Pubkey,
) {
    let (mut svm, creator, task) = setup_open_task();
    let worker = funded_keypair(&mut svm);
    accept_task(&mut svm, &worker, task);
    (svm, creator, worker, task)
}

fn setup_funded_task() -> (
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_keypair::Keypair,
    Pubkey,
    Pubkey,
) {
    let (mut svm, creator, worker, task) = setup_accepted_task();
    let vault = fund_task(&mut svm, &creator, task);
    (svm, creator, worker, task, vault)
}

fn setup_submitted_task() -> (
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_keypair::Keypair,
    Pubkey,
    Pubkey,
) {
    let (mut svm, creator, worker, task, vault) = setup_funded_task();
    submit_task(&mut svm, &worker, task, SUBMISSION_REFERENCE);
    (svm, creator, worker, task, vault)
}

#[test]
fn creator_cancels_open_task() {
    let (mut svm, creator, task_address) = setup_open_task();
    let task_before = fetch_task(&svm, &task_address);
    let task_lamports_before = balance(&svm, &task_address);
    let creator_before = balance(&svm, &creator.pubkey());
    let vault = vault_pda(&task_address).0;

    let metadata = send_instruction(
        &mut svm,
        &creator,
        cancel_task_instruction(creator.pubkey(), task_address),
        &[],
    )
    .unwrap();

    let task = fetch_task(&svm, &task_address);
    assert!(task.status == TaskStatus::Cancelled);
    assert_task_fields_unchanged(&task_before, &task);
    assert_eq!(balance(&svm, &task_address), task_lamports_before);
    assert_eq!(
        creator_before - balance(&svm, &creator.pubkey()),
        metadata.fee
    );
    assert_account_absent(&svm, &vault);
}

#[test]
fn creator_cancels_accepted_task() {
    let (mut svm, creator, worker, task_address) = setup_accepted_task();
    let task_before = fetch_task(&svm, &task_address);
    let creator_before = balance(&svm, &creator.pubkey());
    let worker_before = balance(&svm, &worker.pubkey());
    let vault = vault_pda(&task_address).0;

    let metadata = send_instruction(
        &mut svm,
        &creator,
        cancel_task_instruction(creator.pubkey(), task_address),
        &[],
    )
    .unwrap();

    let task = fetch_task(&svm, &task_address);
    assert!(task.status == TaskStatus::Cancelled);
    assert_task_fields_unchanged(&task_before, &task);
    assert_eq!(task.worker, Some(worker.pubkey()));
    assert_eq!(balance(&svm, &worker.pubkey()), worker_before);
    assert_eq!(
        creator_before - balance(&svm, &creator.pubkey()),
        metadata.fee
    );
    assert_account_absent(&svm, &vault);
}

#[test]
fn random_signer_cannot_cancel() {
    let (mut svm, creator, task) = setup_open_task();
    let attacker = funded_keypair(&mut svm);
    let creator_before = balance(&svm, &creator.pubkey());

    assert_failing_cancel(
        &mut svm,
        &attacker,
        cancel_task_instruction(attacker.pubkey(), task),
        task,
        vault_pda(&task).0,
        |error| assert_task_marketplace_error(error, ErrorCode::Unauthorized),
    );
    assert_eq!(balance(&svm, &creator.pubkey()), creator_before);
}

#[test]
fn worker_cannot_cancel() {
    let (mut svm, creator, worker, task) = setup_accepted_task();
    let creator_before = balance(&svm, &creator.pubkey());

    assert_failing_cancel(
        &mut svm,
        &worker,
        cancel_task_instruction(worker.pubkey(), task),
        task,
        vault_pda(&task).0,
        |error| assert_task_marketplace_error(error, ErrorCode::Unauthorized),
    );
    assert_eq!(balance(&svm, &creator.pubkey()), creator_before);
}

#[test]
fn funded_task_cannot_be_cancelled() {
    let (mut svm, creator, _worker, task, vault) = setup_funded_task();
    assert_invalid_state_cancel(&mut svm, &creator, task, vault);
}

#[test]
fn submitted_task_cannot_be_cancelled() {
    let (mut svm, creator, _worker, task, vault) = setup_submitted_task();
    assert_invalid_state_cancel(&mut svm, &creator, task, vault);
}

#[test]
fn paid_task_cannot_be_cancelled() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    pay_task(&mut svm, &creator, task, vault, worker.pubkey());

    assert_invalid_state_cancel(&mut svm, &creator, task, vault);
    assert_account_absent(&svm, &vault);
}

#[test]
fn cancelled_task_cannot_be_cancelled_again() {
    let (mut svm, creator, task) = setup_open_task();
    send_instruction(
        &mut svm,
        &creator,
        cancel_task_instruction(creator.pubkey(), task),
        &[],
    )
    .unwrap();

    assert_invalid_state_cancel(&mut svm, &creator, task, vault_pda(&task).0);
}

#[test]
fn wrong_task_pda() {
    let (mut svm, creator, task) = setup_open_task();
    let wrong_task = Pubkey::new_unique();
    svm.set_account(wrong_task, svm.get_account(&task).unwrap())
        .unwrap();
    let canonical_task_before = snapshot_account(&svm, &task);

    assert_failing_cancel(
        &mut svm,
        &creator,
        cancel_task_instruction(creator.pubkey(), wrong_task),
        wrong_task,
        vault_pda(&task).0,
        |error| assert_framework_error(error, anchor_lang::error::ErrorCode::ConstraintSeeds),
    );
    assert_account_unchanged(&svm, &task, &canonical_task_before);
}

#[test]
fn missing_creator_signature() {
    let (mut svm, creator, task) = setup_open_task();
    let payer = funded_keypair(&mut svm);
    let creator_before = balance(&svm, &creator.pubkey());
    let mut instruction = cancel_task_instruction(creator.pubkey(), task);
    instruction.accounts[0].is_signer = false;

    assert_failing_cancel(
        &mut svm,
        &payer,
        instruction,
        task,
        vault_pda(&task).0,
        |error| assert_framework_error(error, anchor_lang::error::ErrorCode::AccountNotSigner),
    );
    assert_eq!(balance(&svm, &creator.pubkey()), creator_before);
}

fn assert_invalid_state_cancel(
    svm: &mut litesvm::LiteSVM,
    creator: &solana_keypair::Keypair,
    task: Pubkey,
    vault: Pubkey,
) {
    assert_failing_cancel(
        svm,
        creator,
        cancel_task_instruction(creator.pubkey(), task),
        task,
        vault,
        |error| assert_task_marketplace_error(error, ErrorCode::InvalidStateTransition),
    );
}

fn assert_failing_cancel(
    svm: &mut litesvm::LiteSVM,
    payer: &solana_keypair::Keypair,
    instruction: solana_transaction::Instruction,
    task: Pubkey,
    vault: Pubkey,
    assert_error: impl FnOnce(&litesvm::types::FailedTransactionMetadata),
) {
    let task_before = snapshot_account(svm, &task);
    let vault_before = snapshot_account(svm, &vault);
    let payer_before = balance(svm, &payer.pubkey());

    let error = send_instruction(svm, payer, instruction, &[]).unwrap_err();

    assert_error(&error);
    assert_account_unchanged(svm, &task, &task_before);
    assert_account_unchanged(svm, &vault, &vault_before);
    assert_eq!(payer_before - balance(svm, &payer.pubkey()), error.meta.fee);
    assert_vault_solvent_if_present(svm, &vault);
}

fn assert_task_fields_unchanged(before: &task_marketplace::Task, after: &task_marketplace::Task) {
    assert_eq!(after.task_number, before.task_number);
    assert_eq!(after.creator, before.creator);
    assert_eq!(after.worker, before.worker);
    assert_eq!(after.title, before.title);
    assert_eq!(after.description, before.description);
    assert_eq!(after.reward_amount, before.reward_amount);
    assert_eq!(after.submission_reference, before.submission_reference);
    assert_eq!(after.funded_at, before.funded_at);
    assert_eq!(after.submission_deadline, before.submission_deadline);
    assert_eq!(after.review_deadline, before.review_deadline);
}
