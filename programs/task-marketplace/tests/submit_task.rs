mod common;

use anchor_lang::prelude::Pubkey;
use solana_signer::Signer;
use task_marketplace::{error::ErrorCode, state::TaskStatus};

use common::*;

const SUBMISSION_REFERENCE: &str = "ipfs://integration-test-submission";

fn setup_funded_task() -> (
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
    (svm, creator, worker, task, vault)
}

fn assert_failed_submission_rollback(
    svm: &litesvm::LiteSVM,
    task: Pubkey,
    task_before: &Option<AccountSnapshot>,
    vault: Pubkey,
    vault_before: &Option<AccountSnapshot>,
) {
    assert_account_unchanged(svm, &task, task_before);
    assert_account_unchanged(svm, &vault, vault_before);
    assert_vault_solvent_if_present(svm, &vault);
}

#[test]
fn worker_submits_funded_task() {
    let (mut svm, _creator, worker, task, vault) = setup_funded_task();
    let funded_at = fetch_task(&svm, &task).funded_at;
    let submission_deadline = fetch_task(&svm, &task).submission_deadline;
    let vault_before = snapshot_account(&svm, &vault);

    submit_task(&mut svm, &worker, task, SUBMISSION_REFERENCE);

    let submitted_task = fetch_task(&svm, &task);
    assert!(submitted_task.status == TaskStatus::Submitted);
    assert_eq!(
        submitted_task.submission_reference,
        Some(SUBMISSION_REFERENCE.to_string())
    );
    assert_eq!(submitted_task.worker, Some(worker.pubkey()));
    assert_eq!(submitted_task.funded_at, funded_at);
    assert_eq!(submitted_task.submission_deadline, submission_deadline);
    assert!(submitted_task.review_deadline.is_some());
    assert_account_unchanged(&svm, &vault, &vault_before);
    assert_vault_solvent(&svm, &vault);
}

#[test]
fn submission_deadline_boundary_is_enforced() {
    let (mut svm, _creator, worker, task, vault) = setup_funded_task();
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline - 1);

    submit_task(&mut svm, &worker, task, SUBMISSION_REFERENCE);
    assert!(fetch_task(&svm, &task).status == TaskStatus::Submitted);
    assert_vault_solvent(&svm, &vault);

    let (mut svm, _creator, worker, task, vault) = setup_funded_task();
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);
    assert_failing_submission(
        &mut svm,
        &worker,
        task,
        vault,
        ErrorCode::SubmissionWindowExpired,
    );
}

#[test]
fn wrong_worker() {
    let (mut svm, _creator, _worker, task, vault) = setup_funded_task();
    let wrong_worker = funded_keypair(&mut svm);
    assert_failing_submission(
        &mut svm,
        &wrong_worker,
        task,
        vault,
        ErrorCode::Unauthorized,
    );
}

#[test]
fn creator_submits() {
    let (mut svm, creator, _worker, task, vault) = setup_funded_task();
    assert_failing_submission(&mut svm, &creator, task, vault, ErrorCode::Unauthorized);
}

#[test]
fn open_task() {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let worker = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    let vault = vault_pda(&task).0;

    assert_failing_submission(
        &mut svm,
        &worker,
        task,
        vault,
        ErrorCode::InvalidStateTransition,
    );
    assert_account_absent(&svm, &vault);
}

#[test]
fn accepted_task() {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let worker = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    accept_task(&mut svm, &worker, task);
    let vault = vault_pda(&task).0;

    assert_failing_submission(
        &mut svm,
        &worker,
        task,
        vault,
        ErrorCode::InvalidStateTransition,
    );
    assert_account_absent(&svm, &vault);
}

#[test]
fn replay_submission() {
    let (mut svm, _creator, worker, task, vault) = setup_funded_task();
    submit_task(&mut svm, &worker, task, SUBMISSION_REFERENCE);
    let task_before = snapshot_account(&svm, &task);
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &worker,
        submit_task_instruction(worker.pubkey(), task, "ipfs://replacement".to_string()),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::InvalidStateTransition);
    assert_failed_submission_rollback(&svm, task, &task_before, vault, &vault_before);
}

#[test]
fn missing_signature() {
    let (mut svm, _creator, worker, task, vault) = setup_funded_task();
    let payer = funded_keypair(&mut svm);
    let task_before = snapshot_account(&svm, &task);
    let vault_before = snapshot_account(&svm, &vault);
    let mut instruction =
        submit_task_instruction(worker.pubkey(), task, SUBMISSION_REFERENCE.to_string());
    instruction.accounts[0].is_signer = false;

    let error = send_instruction(&mut svm, &payer, instruction, &[]).unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::AccountNotSigner);
    assert_failed_submission_rollback(&svm, task, &task_before, vault, &vault_before);
}

#[test]
fn wrong_pda() {
    let (mut svm, _creator, worker, task, vault) = setup_funded_task();
    let wrong_task = Pubkey::new_unique();
    svm.set_account(wrong_task, svm.get_account(&task).unwrap())
        .unwrap();
    let task_before = snapshot_account(&svm, &task);
    let wrong_task_before = snapshot_account(&svm, &wrong_task);
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &worker,
        submit_task_instruction(
            worker.pubkey(),
            wrong_task,
            SUBMISSION_REFERENCE.to_string(),
        ),
        &[],
    )
    .unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_unchanged(&svm, &task, &task_before);
    assert_account_unchanged(&svm, &wrong_task, &wrong_task_before);
    assert_account_unchanged(&svm, &vault, &vault_before);
    assert_vault_solvent(&svm, &vault);
}

fn assert_failing_submission(
    svm: &mut litesvm::LiteSVM,
    signer: &solana_keypair::Keypair,
    task: Pubkey,
    vault: Pubkey,
    expected_error: ErrorCode,
) {
    let task_before = snapshot_account(svm, &task);
    let vault_before = snapshot_account(svm, &vault);

    let error = send_instruction(
        svm,
        signer,
        submit_task_instruction(signer.pubkey(), task, SUBMISSION_REFERENCE.to_string()),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, expected_error);
    assert_failed_submission_rollback(svm, task, &task_before, vault, &vault_before);
}
