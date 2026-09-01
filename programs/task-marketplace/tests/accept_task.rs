mod common;

use anchor_lang::prelude::Pubkey;
use solana_signer::Signer;
use task_marketplace::{error::ErrorCode, state::TaskStatus};

use common::*;

fn setup_open_task() -> (
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
    (svm, creator, worker, task)
}

fn assert_failed_acceptance_rollback(
    svm: &litesvm::LiteSVM,
    task: Pubkey,
    task_before: &Option<AccountSnapshot>,
    vault_before: &Option<AccountSnapshot>,
) {
    assert_account_unchanged(svm, &task, task_before);
    assert_account_unchanged(svm, &vault_pda(&task).0, vault_before);
}

#[test]
fn worker_accepts_open_task() {
    let (mut svm, _creator, worker, task_address) = setup_open_task();

    accept_task(&mut svm, &worker, task_address);

    let task = fetch_task(&svm, &task_address);
    assert_eq!(task.worker, Some(worker.pubkey()));
    assert!(task.status == TaskStatus::Accepted);
    assert_eq!(task.funded_at, None);
    assert_account_absent(&svm, &vault_pda(&task_address).0);
}

#[test]
fn creator_cannot_self_accept() {
    let (mut svm, creator, _worker, task) = setup_open_task();
    let task_before = snapshot_account(&svm, &task);
    let vault = vault_pda(&task).0;
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &creator,
        accept_task_instruction(creator.pubkey(), task),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::Unauthorized);
    assert_failed_acceptance_rollback(&svm, task, &task_before, &vault_before);
    assert_account_absent(&svm, &vault);
}

#[test]
fn replay_acceptance() {
    let (mut svm, _creator, worker, task) = setup_open_task();
    accept_task(&mut svm, &worker, task);
    let task_before = snapshot_account(&svm, &task);
    let vault = vault_pda(&task).0;
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &worker,
        accept_task_instruction(worker.pubkey(), task),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::InvalidStateTransition);
    assert_failed_acceptance_rollback(&svm, task, &task_before, &vault_before);
    assert_account_absent(&svm, &vault);
}

#[test]
fn second_worker_cannot_overwrite() {
    let (mut svm, _creator, first_worker, task) = setup_open_task();
    let second_worker = funded_keypair(&mut svm);
    accept_task(&mut svm, &first_worker, task);
    let task_before = snapshot_account(&svm, &task);
    let vault = vault_pda(&task).0;
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &second_worker,
        accept_task_instruction(second_worker.pubkey(), task),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::InvalidStateTransition);
    assert_failed_acceptance_rollback(&svm, task, &task_before, &vault_before);
    assert_eq!(fetch_task(&svm, &task).worker, Some(first_worker.pubkey()));
    assert_account_absent(&svm, &vault);
}

#[test]
fn missing_worker_signature() {
    let (mut svm, _creator, worker, task) = setup_open_task();
    let payer = funded_keypair(&mut svm);
    let task_before = snapshot_account(&svm, &task);
    let vault = vault_pda(&task).0;
    let vault_before = snapshot_account(&svm, &vault);
    let mut instruction = accept_task_instruction(worker.pubkey(), task);
    instruction.accounts[0].is_signer = false;

    let error = send_instruction(&mut svm, &payer, instruction, &[]).unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::AccountNotSigner);
    assert_failed_acceptance_rollback(&svm, task, &task_before, &vault_before);
    assert_account_absent(&svm, &vault);
}

#[test]
fn wrong_task_pda() {
    let (mut svm, _creator, worker, task) = setup_open_task();
    let wrong_task = Pubkey::new_unique();
    let copied_account = svm.get_account(&task).unwrap();
    svm.set_account(wrong_task, copied_account).unwrap();
    let task_before = snapshot_account(&svm, &task);
    let wrong_task_before = snapshot_account(&svm, &wrong_task);
    let vault = vault_pda(&wrong_task).0;
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &worker,
        accept_task_instruction(worker.pubkey(), wrong_task),
        &[],
    )
    .unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_unchanged(&svm, &task, &task_before);
    assert_account_unchanged(&svm, &wrong_task, &wrong_task_before);
    assert_account_unchanged(&svm, &vault, &vault_before);
    assert_account_absent(&svm, &vault);
}

#[test]
fn race_simulation_worker_a_wins() {
    assert_race_winner(true);
}

#[test]
fn race_simulation_worker_b_wins() {
    assert_race_winner(false);
}

fn assert_race_winner(worker_a_first: bool) {
    let (mut svm, _creator, worker_a, task) = setup_open_task();
    let worker_b = funded_keypair(&mut svm);
    let (winner, loser) = if worker_a_first {
        (&worker_a, &worker_b)
    } else {
        (&worker_b, &worker_a)
    };

    accept_task(&mut svm, winner, task);
    let task_after_winner = snapshot_account(&svm, &task);
    let vault = vault_pda(&task).0;
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        loser,
        accept_task_instruction(loser.pubkey(), task),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::InvalidStateTransition);
    assert_failed_acceptance_rollback(&svm, task, &task_after_winner, &vault_before);
    assert_eq!(fetch_task(&svm, &task).worker, Some(winner.pubkey()));
    assert_account_absent(&svm, &vault);
}
