mod common;

use anchor_lang::prelude::Pubkey;
use solana_signer::Signer;
use task_marketplace::{
    error::ErrorCode,
    state::{EscrowVault, TaskStatus},
};

use common::*;

const SUBMISSION_REFERENCE: &str = "ipfs://timeout-settlement";

fn setup_submitted_task() -> (
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
    set_clock_timestamp(&mut svm, 1_001);
    submit_task(&mut svm, &worker, task, SUBMISSION_REFERENCE);
    (svm, creator, worker, task, vault)
}

#[test]
fn arbitrary_payer_settles_at_review_deadline() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let settler = funded_keypair(&mut svm);
    let task_before = fetch_task(&svm, &task);
    let deadline = task_before.review_deadline.unwrap();
    let worker_before = balance(&svm, &worker.pubkey());
    let creator_before = balance(&svm, &creator.pubkey());
    let settler_before = balance(&svm, &settler.pubkey());
    let vault_before = balance(&svm, &vault);
    let worker_amount = fetch_vault(&svm, &vault).escrowed_lamports;
    set_clock_timestamp(&mut svm, deadline);

    let metadata = send_instruction(
        &mut svm,
        &settler,
        settle_task_after_timeout_instruction(
            settler.pubkey(),
            task,
            creator.pubkey(),
            worker.pubkey(),
            vault,
        ),
        &[],
    )
    .unwrap();

    let paid_task = fetch_task(&svm, &task);
    assert!(paid_task.status == TaskStatus::Paid);
    assert_eq!(paid_task.worker, task_before.worker);
    assert_eq!(paid_task.funded_at, task_before.funded_at);
    assert_eq!(
        paid_task.submission_reference,
        task_before.submission_reference
    );
    assert_eq!(
        paid_task.submission_deadline,
        task_before.submission_deadline
    );
    assert_eq!(paid_task.review_deadline, Some(deadline));
    assert_eq!(
        balance(&svm, &worker.pubkey()),
        worker_before + worker_amount
    );
    assert_eq!(
        balance(&svm, &creator.pubkey()) - creator_before,
        vault_before - worker_amount
    );
    assert_eq!(
        settler_before - balance(&svm, &settler.pubkey()),
        metadata.fee
    );
    assert_account_absent(&svm, &vault);
}

#[test]
fn settlement_before_review_deadline_fails_atomically() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let settler = funded_keypair(&mut svm);
    let deadline = fetch_task(&svm, &task).review_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline - 1);

    assert_failing_settlement(
        &mut svm,
        &settler,
        task,
        creator.pubkey(),
        worker.pubkey(),
        vault,
        |error| assert_task_marketplace_error(error, ErrorCode::ReviewDeadlineNotReached),
    );
}

#[test]
fn wrong_creator_or_worker_is_rejected() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let settler = funded_keypair(&mut svm);
    let wrong = funded_keypair(&mut svm);
    let deadline = fetch_task(&svm, &task).review_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);

    assert_failing_settlement(
        &mut svm,
        &settler,
        task,
        wrong.pubkey(),
        worker.pubkey(),
        vault,
        |error| assert_task_marketplace_error(error, ErrorCode::Unauthorized),
    );
    assert_failing_settlement(
        &mut svm,
        &settler,
        task,
        creator.pubkey(),
        wrong.pubkey(),
        vault,
        |error| assert_task_marketplace_error(error, ErrorCode::Unauthorized),
    );
}

#[test]
fn settlement_replay_fails() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let settler = funded_keypair(&mut svm);
    let deadline = fetch_task(&svm, &task).review_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);
    send_instruction(
        &mut svm,
        &settler,
        settle_task_after_timeout_instruction(
            settler.pubkey(),
            task,
            creator.pubkey(),
            worker.pubkey(),
            vault,
        ),
        &[],
    )
    .unwrap();
    let task_before = snapshot_account(&svm, &task);
    let worker_before = balance(&svm, &worker.pubkey());

    let error = send_instruction(
        &mut svm,
        &settler,
        settle_task_after_timeout_instruction(
            settler.pubkey(),
            task,
            creator.pubkey(),
            worker.pubkey(),
            vault,
        ),
        &[],
    )
    .unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::AccountNotInitialized);
    assert_account_unchanged(&svm, &task, &task_before);
    assert_account_absent(&svm, &vault);
    assert_eq!(balance(&svm, &worker.pubkey()), worker_before);
}

#[test]
fn wrong_vault_pda_is_rejected() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let settler = funded_keypair(&mut svm);
    let wrong_vault = Pubkey::new_unique();
    svm.set_account(wrong_vault, svm.get_account(&vault).unwrap())
        .unwrap();
    let deadline = fetch_task(&svm, &task).review_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);

    assert_failing_settlement(
        &mut svm,
        &settler,
        task,
        creator.pubkey(),
        worker.pubkey(),
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
        (
            (|vault: &mut EscrowVault| {
                vault.escrowed_lamports = vault.escrowed_lamports.checked_add(1).unwrap()
            }) as fn(&mut EscrowVault),
            ErrorCode::InvalidEscrowLiability,
        ),
    ] {
        let (mut svm, creator, worker, task, vault_address) = setup_submitted_task();
        let settler = funded_keypair(&mut svm);
        let mut vault = fetch_vault(&svm, &vault_address);
        mutate(&mut vault);
        overwrite_vault(&mut svm, vault_address, &vault);
        let deadline = fetch_task(&svm, &task).review_deadline.unwrap();
        set_clock_timestamp(&mut svm, deadline);

        assert_failing_settlement(
            &mut svm,
            &settler,
            task,
            creator.pubkey(),
            worker.pubkey(),
            vault_address,
            |error| assert_task_marketplace_error(error, expected),
        );
    }
}

#[test]
fn insolvent_vault_is_rejected() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let settler = funded_keypair(&mut svm);
    let escrowed = fetch_vault(&svm, &vault).escrowed_lamports;
    let rent = svm.minimum_balance_for_rent_exemption(8 + 106);
    set_balance(&mut svm, vault, rent + escrowed - 1);
    let deadline = fetch_task(&svm, &task).review_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);

    assert_failing_settlement(
        &mut svm,
        &settler,
        task,
        creator.pubkey(),
        worker.pubkey(),
        vault,
        |error| assert_task_marketplace_error(error, ErrorCode::EscrowBalanceMismatch),
    );
}

#[test]
fn donation_surplus_goes_to_creator() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let settler = funded_keypair(&mut svm);
    let donor = funded_keypair(&mut svm);
    let donation = 123_456;
    transfer_lamports(&mut svm, &donor, vault, donation);
    let creator_before = balance(&svm, &creator.pubkey());
    let vault_before = balance(&svm, &vault);
    let worker_amount = fetch_vault(&svm, &vault).escrowed_lamports;
    let deadline = fetch_task(&svm, &task).review_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);

    send_instruction(
        &mut svm,
        &settler,
        settle_task_after_timeout_instruction(
            settler.pubkey(),
            task,
            creator.pubkey(),
            worker.pubkey(),
            vault,
        ),
        &[],
    )
    .unwrap();

    assert_eq!(
        balance(&svm, &creator.pubkey()) - creator_before,
        vault_before - worker_amount
    );
    assert_account_absent(&svm, &vault);
}

#[test]
fn missing_review_deadline_is_rejected() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let settler = funded_keypair(&mut svm);
    let mut task_account = fetch_task(&svm, &task);
    task_account.review_deadline = None;
    overwrite_task(&mut svm, task, &task_account);

    assert_failing_settlement(
        &mut svm,
        &settler,
        task,
        creator.pubkey(),
        worker.pubkey(),
        vault,
        |error| assert_task_marketplace_error(error, ErrorCode::InvalidStateTransition),
    );
}

#[test]
fn creator_payment_and_timeout_settlement_races_are_safe() {
    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let settler = funded_keypair(&mut svm);
    let deadline = fetch_task(&svm, &task).review_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);
    pay_task(&mut svm, &creator, task, vault, worker.pubkey());
    let task_before = snapshot_account(&svm, &task);
    let worker_before = balance(&svm, &worker.pubkey());
    let error = send_instruction(
        &mut svm,
        &settler,
        settle_task_after_timeout_instruction(
            settler.pubkey(),
            task,
            creator.pubkey(),
            worker.pubkey(),
            vault,
        ),
        &[],
    )
    .unwrap_err();
    assert_framework_error(&error, anchor_lang::error::ErrorCode::AccountNotInitialized);
    assert_account_unchanged(&svm, &task, &task_before);
    assert_eq!(balance(&svm, &worker.pubkey()), worker_before);

    let (mut svm, creator, worker, task, vault) = setup_submitted_task();
    let settler = funded_keypair(&mut svm);
    let deadline = fetch_task(&svm, &task).review_deadline.unwrap();
    set_clock_timestamp(&mut svm, deadline);
    send_instruction(
        &mut svm,
        &settler,
        settle_task_after_timeout_instruction(
            settler.pubkey(),
            task,
            creator.pubkey(),
            worker.pubkey(),
            vault,
        ),
        &[],
    )
    .unwrap();
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
    assert_eq!(balance(&svm, &worker.pubkey()), worker_before);
}

fn assert_failing_settlement(
    svm: &mut litesvm::LiteSVM,
    payer: &solana_keypair::Keypair,
    task: Pubkey,
    creator: Pubkey,
    worker: Pubkey,
    vault: Pubkey,
    assert_error: impl FnOnce(&litesvm::types::FailedTransactionMetadata),
) {
    let task_before = snapshot_account(svm, &task);
    let vault_before = snapshot_account(svm, &vault);
    let creator_before = balance(svm, &creator);
    let worker_before = balance(svm, &worker);
    let payer_before = balance(svm, &payer.pubkey());

    let error = send_instruction(
        svm,
        payer,
        settle_task_after_timeout_instruction(payer.pubkey(), task, creator, worker, vault),
        &[],
    )
    .unwrap_err();

    assert_error(&error);
    assert_account_unchanged(svm, &task, &task_before);
    assert_account_unchanged(svm, &vault, &vault_before);
    assert_eq!(balance(svm, &creator), creator_before);
    assert_eq!(balance(svm, &worker), worker_before);
    assert_eq!(payer_before - balance(svm, &payer.pubkey()), error.meta.fee);
    assert_vault_solvency_preserved_if_initially_solvent(svm, &vault, &vault_before);
}
