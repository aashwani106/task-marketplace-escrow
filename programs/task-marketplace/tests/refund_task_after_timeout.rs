mod common;

use anchor_lang::prelude::Pubkey;
use solana_signer::Signer;
use task_marketplace::{
    error::ErrorCode,
    state::{EscrowVault, TaskStatus},
};

use common::*;

const SUBMISSION_REFERENCE: &str = "ipfs://refund-race-submission";

fn setup_funded_task() -> (
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_keypair::Keypair,
    Pubkey,
    Pubkey,
) {
    let mut svm = bootstrap();
    set_clock_timestamp(&mut svm, 1_000);
    let creator = funded_keypair(&mut svm);
    let worker = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    accept_task(&mut svm, &worker, task);
    let vault = fund_task(&mut svm, &creator, task);
    (svm, creator, worker, task, vault)
}

#[test]
fn creator_refunds_at_submission_deadline() {
    let (mut svm, creator, worker, task, vault) = setup_funded_task();
    let task_before = fetch_task(&svm, &task);
    let deadline = task_before.submission_deadline.unwrap();
    let creator_before = balance(&svm, &creator.pubkey());
    let vault_before = balance(&svm, &vault);
    set_clock_timestamp(&mut svm, deadline);

    let metadata = send_instruction(
        &mut svm,
        &creator,
        refund_task_after_timeout_instruction(creator.pubkey(), task, vault),
        &[],
    )
    .unwrap();

    let refunded_task = fetch_task(&svm, &task);
    assert!(refunded_task.status == TaskStatus::Cancelled);
    assert_eq!(refunded_task.creator, task_before.creator);
    assert_eq!(refunded_task.worker, Some(worker.pubkey()));
    assert_eq!(refunded_task.reward_amount, task_before.reward_amount);
    assert_eq!(refunded_task.funded_at, task_before.funded_at);
    assert_eq!(refunded_task.submission_reference, None);
    assert_eq!(refunded_task.submission_deadline, Some(deadline));
    assert_eq!(refunded_task.review_deadline, None);
    assert_eq!(
        balance(&svm, &creator.pubkey()) + metadata.fee - creator_before,
        vault_before
    );
    assert_account_absent(&svm, &vault);
}

#[test]
fn refund_before_deadline_fails_atomically() {
    let (mut svm, creator, _worker, task, vault) = setup_funded_task();
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline - 1);

    assert_failing_refund(&mut svm, &creator, creator.pubkey(), task, vault, |error| {
        assert_task_marketplace_error(error, ErrorCode::SubmissionDeadlineNotReached)
    });
}

#[test]
fn non_creator_cannot_refund() {
    let (mut svm, _creator, _worker, task, vault) = setup_funded_task();
    let attacker = funded_keypair(&mut svm);
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);

    assert_failing_refund(
        &mut svm,
        &attacker,
        attacker.pubkey(),
        task,
        vault,
        |error| assert_task_marketplace_error(error, ErrorCode::Unauthorized),
    );
}

#[test]
fn creator_signature_is_required() {
    let (mut svm, creator, _worker, task, vault) = setup_funded_task();
    let payer = funded_keypair(&mut svm);
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);
    let task_before = snapshot_account(&svm, &task);
    let vault_before = snapshot_account(&svm, &vault);
    let mut instruction = refund_task_after_timeout_instruction(creator.pubkey(), task, vault);
    instruction.accounts[0].is_signer = false;

    let error = send_instruction(&mut svm, &payer, instruction, &[]).unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::AccountNotSigner);
    assert_account_unchanged(&svm, &task, &task_before);
    assert_account_unchanged(&svm, &vault, &vault_before);
    assert_vault_solvent(&svm, &vault);
}

#[test]
fn submitted_task_cannot_be_refunded() {
    let (mut svm, creator, worker, task, vault) = setup_funded_task();
    submit_task(&mut svm, &worker, task, SUBMISSION_REFERENCE);
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);

    assert_failing_refund(&mut svm, &creator, creator.pubkey(), task, vault, |error| {
        assert_task_marketplace_error(error, ErrorCode::InvalidStateTransition)
    });
}

#[test]
fn refund_replay_fails() {
    let (mut svm, creator, _worker, task, vault) = setup_funded_task();
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);
    send_instruction(
        &mut svm,
        &creator,
        refund_task_after_timeout_instruction(creator.pubkey(), task, vault),
        &[],
    )
    .unwrap();
    let task_before = snapshot_account(&svm, &task);

    let error = send_instruction(
        &mut svm,
        &creator,
        refund_task_after_timeout_instruction(creator.pubkey(), task, vault),
        &[],
    )
    .unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::AccountNotInitialized);
    assert_account_unchanged(&svm, &task, &task_before);
    assert_account_absent(&svm, &vault);
}

#[test]
fn wrong_vault_pda_is_rejected() {
    let (mut svm, creator, _worker, task, vault) = setup_funded_task();
    let wrong_vault = Pubkey::new_unique();
    svm.set_account(wrong_vault, svm.get_account(&vault).unwrap())
        .unwrap();
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);

    assert_failing_refund(
        &mut svm,
        &creator,
        creator.pubkey(),
        task,
        wrong_vault,
        |error| assert_framework_error(error, anchor_lang::error::ErrorCode::ConstraintSeeds),
    );
    assert_vault_solvent(&svm, &vault);
}

#[test]
fn invalid_vault_metadata_is_rejected() {
    for (mutate, expected) in [
        (
            (|vault: &mut EscrowVault| vault.version = vault.version.checked_add(1).unwrap())
                as fn(&mut EscrowVault),
            ErrorCode::InvalidVaultVersion,
        ),
        (
            (|vault: &mut EscrowVault| vault.task = Pubkey::new_unique()) as fn(&mut EscrowVault),
            ErrorCode::InvalidVaultTask,
        ),
    ] {
        let (mut svm, creator, _worker, task, vault_address) = setup_funded_task();
        let mut vault = fetch_vault(&svm, &vault_address);
        mutate(&mut vault);
        overwrite_vault(&mut svm, vault_address, &vault);
        let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
        set_clock_timestamp(&mut svm, deadline);

        assert_failing_refund(
            &mut svm,
            &creator,
            creator.pubkey(),
            task,
            vault_address,
            |error| assert_task_marketplace_error(error, expected),
        );
    }
}

#[test]
fn liability_mismatch_is_rejected() {
    let (mut svm, creator, _worker, task, vault_address) = setup_funded_task();
    let mut vault = fetch_vault(&svm, &vault_address);
    vault.escrowed_lamports = vault.escrowed_lamports.checked_add(1).unwrap();
    overwrite_vault(&mut svm, vault_address, &vault);
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);

    assert_failing_refund(
        &mut svm,
        &creator,
        creator.pubkey(),
        task,
        vault_address,
        |error| assert_task_marketplace_error(error, ErrorCode::InvalidEscrowLiability),
    );
}

#[test]
fn insolvent_vault_is_rejected() {
    let (mut svm, creator, _worker, task, vault) = setup_funded_task();
    let escrowed = fetch_vault(&svm, &vault).escrowed_lamports;
    let rent = svm.minimum_balance_for_rent_exemption(8 + 106);
    set_balance(&mut svm, vault, rent + escrowed - 1);
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);

    assert_failing_refund(&mut svm, &creator, creator.pubkey(), task, vault, |error| {
        assert_task_marketplace_error(error, ErrorCode::EscrowBalanceMismatch)
    });
}

#[test]
fn donation_is_returned_to_creator() {
    let (mut svm, creator, _worker, task, vault) = setup_funded_task();
    let donor = funded_keypair(&mut svm);
    let donation = 123_456;
    transfer_lamports(&mut svm, &donor, vault, donation);
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    let creator_before = balance(&svm, &creator.pubkey());
    let vault_before = balance(&svm, &vault);
    set_clock_timestamp(&mut svm, deadline);

    let metadata = send_instruction(
        &mut svm,
        &creator,
        refund_task_after_timeout_instruction(creator.pubkey(), task, vault),
        &[],
    )
    .unwrap();

    assert_eq!(
        balance(&svm, &creator.pubkey()) + metadata.fee - creator_before,
        vault_before
    );
    assert_account_absent(&svm, &vault);
}

#[test]
fn submission_and_refund_race_has_one_winner_in_each_order() {
    let (mut svm, creator, worker, task, vault) = setup_funded_task();
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline - 1);
    submit_task(&mut svm, &worker, task, SUBMISSION_REFERENCE);
    set_clock_timestamp(&mut svm, deadline);
    assert_failing_refund(&mut svm, &creator, creator.pubkey(), task, vault, |error| {
        assert_task_marketplace_error(error, ErrorCode::InvalidStateTransition)
    });

    let (mut svm, creator, worker, task, vault) = setup_funded_task();
    let deadline = fetch_task(&svm, &task).submission_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);
    send_instruction(
        &mut svm,
        &creator,
        refund_task_after_timeout_instruction(creator.pubkey(), task, vault),
        &[],
    )
    .unwrap();
    let task_before = snapshot_account(&svm, &task);
    let error = send_instruction(
        &mut svm,
        &worker,
        submit_task_instruction(worker.pubkey(), task, SUBMISSION_REFERENCE.to_string()),
        &[],
    )
    .unwrap_err();
    assert_task_marketplace_error(&error, ErrorCode::InvalidStateTransition);
    assert_account_unchanged(&svm, &task, &task_before);
    assert_account_absent(&svm, &vault);
}

fn assert_failing_refund(
    svm: &mut litesvm::LiteSVM,
    payer: &solana_keypair::Keypair,
    creator: Pubkey,
    task: Pubkey,
    vault: Pubkey,
    assert_error: impl FnOnce(&litesvm::types::FailedTransactionMetadata),
) {
    let task_before = snapshot_account(svm, &task);
    let vault_before = snapshot_account(svm, &vault);
    let payer_before = balance(svm, &payer.pubkey());

    let error = send_instruction(
        svm,
        payer,
        refund_task_after_timeout_instruction(creator, task, vault),
        &[],
    )
    .unwrap_err();

    assert_error(&error);
    assert_account_unchanged(svm, &task, &task_before);
    assert_account_unchanged(svm, &vault, &vault_before);
    assert_eq!(payer_before - balance(svm, &payer.pubkey()), error.meta.fee);
    assert_vault_solvency_preserved_if_initially_solvent(svm, &vault, &vault_before);
}
