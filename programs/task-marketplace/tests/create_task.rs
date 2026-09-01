mod common;

use solana_signer::Signer;
use task_marketplace::{error::ErrorCode, state::TaskStatus};

use common::*;

fn setup_creator() -> (
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    anchor_lang::prelude::Pubkey,
) {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let profile = create_creator_profile(&mut svm, &creator);
    (svm, creator, profile)
}

fn assert_failed_creation_rollback(
    svm: &litesvm::LiteSVM,
    profile: anchor_lang::prelude::Pubkey,
    profile_before: &Option<AccountSnapshot>,
    task: anchor_lang::prelude::Pubkey,
    vault_before: &Option<AccountSnapshot>,
) {
    assert_account_unchanged(svm, &profile, profile_before);
    assert_account_absent(svm, &task);
    assert_account_unchanged(svm, &vault_pda(&task).0, vault_before);
}

#[test]
fn successful_task_creation() {
    let (mut svm, creator, profile) = setup_creator();
    let task_address = create_task(&mut svm, &creator, 1);

    let profile_account = fetch_creator_profile(&svm, &profile);
    let task = fetch_task(&svm, &task_address);
    assert_eq!(profile_account.task_count, 1);
    assert_eq!(task.task_number, 1);
    assert_eq!(task.creator, creator.pubkey());
    assert_eq!(task.worker, None);
    assert_eq!(task.title, DEFAULT_TITLE);
    assert_eq!(task.description, DEFAULT_DESCRIPTION);
    assert_eq!(task.reward_amount, DEFAULT_REWARD);
    assert!(task.status == TaskStatus::Open);
    assert_eq!(task.submission_reference, None);
    assert_eq!(task.funded_at, None);
    assert_eq!(task.submission_deadline, None);
    assert_eq!(task.review_deadline, None);
    assert_eq!(svm.get_account(&task_address).unwrap().data.len(), 8 + 922);
}

#[test]
fn sequential_task_creation() {
    let (mut svm, creator, profile) = setup_creator();
    let first = create_task(&mut svm, &creator, 1);
    let second = create_task(&mut svm, &creator, 2);

    assert_eq!(fetch_creator_profile(&svm, &profile).task_count, 2);
    assert_eq!(fetch_task(&svm, &first).task_number, 1);
    assert_eq!(fetch_task(&svm, &second).task_number, 2);
}

#[test]
fn duplicate_task_number() {
    let (mut svm, creator, profile) = setup_creator();
    let task = create_task(&mut svm, &creator, 1);
    let profile_before = snapshot_account(&svm, &profile);
    let task_before = snapshot_account(&svm, &task);
    let vault = vault_pda(&task).0;
    let vault_before = snapshot_account(&svm, &vault);

    let error = send_instruction(
        &mut svm,
        &creator,
        create_task_instruction(
            creator.pubkey(),
            profile,
            task,
            1,
            DEFAULT_TITLE.to_string(),
            DEFAULT_DESCRIPTION.to_string(),
            DEFAULT_REWARD,
        ),
        &[],
    )
    .unwrap_err();

    assert_custom_error(&error, 0);
    assert_account_unchanged(&svm, &profile, &profile_before);
    assert_account_unchanged(&svm, &task, &task_before);
    assert_account_unchanged(&svm, &vault, &vault_before);
}

#[test]
fn skipped_task_number() {
    assert_invalid_new_task(
        2,
        DEFAULT_TITLE,
        DEFAULT_DESCRIPTION,
        DEFAULT_REWARD,
        |error| {
            assert_task_marketplace_error(error, ErrorCode::InvalidTaskNumber);
        },
    );
}

#[test]
fn zero_reward() {
    assert_invalid_new_task(1, DEFAULT_TITLE, DEFAULT_DESCRIPTION, 0, |error| {
        assert_task_marketplace_error(error, ErrorCode::InvalidReward);
    });
}

#[test]
fn empty_title() {
    assert_invalid_new_task(1, "", DEFAULT_DESCRIPTION, DEFAULT_REWARD, |error| {
        assert_task_marketplace_error(error, ErrorCode::InvalidTitle);
    });
}

#[test]
fn empty_description() {
    assert_invalid_new_task(1, DEFAULT_TITLE, "", DEFAULT_REWARD, |error| {
        assert_task_marketplace_error(error, ErrorCode::InvalidDescription);
    });
}

#[test]
fn oversized_title() {
    let title = "t".repeat(101);
    assert_invalid_new_task(1, &title, DEFAULT_DESCRIPTION, DEFAULT_REWARD, |error| {
        assert_task_marketplace_error(error, ErrorCode::InvalidTitle);
    });
}

#[test]
fn oversized_description() {
    let description = "d".repeat(501);
    assert_invalid_new_task(1, DEFAULT_TITLE, &description, DEFAULT_REWARD, |error| {
        assert_task_marketplace_error(error, ErrorCode::InvalidDescription);
    });
}

#[test]
fn wrong_creator_profile() {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let other_creator = funded_keypair(&mut svm);
    let creator_profile = create_creator_profile(&mut svm, &creator);
    let other_profile = create_creator_profile(&mut svm, &other_creator);
    let (task, _) = task_pda(&creator.pubkey(), 1);
    let creator_profile_before = snapshot_account(&svm, &creator_profile);
    let other_profile_before = snapshot_account(&svm, &other_profile);
    let vault_before = snapshot_account(&svm, &vault_pda(&task).0);

    let error = send_instruction(
        &mut svm,
        &creator,
        create_task_instruction(
            creator.pubkey(),
            other_profile,
            task,
            1,
            DEFAULT_TITLE.to_string(),
            DEFAULT_DESCRIPTION.to_string(),
            DEFAULT_REWARD,
        ),
        &[],
    )
    .unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_unchanged(&svm, &creator_profile, &creator_profile_before);
    assert_account_unchanged(&svm, &other_profile, &other_profile_before);
    assert_account_absent(&svm, &task);
    assert_account_unchanged(&svm, &vault_pda(&task).0, &vault_before);
}

#[test]
fn cross_creator_task_pda_substitution() {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let other_creator = funded_keypair(&mut svm);
    let profile = create_creator_profile(&mut svm, &creator);
    let (expected_task, _) = task_pda(&creator.pubkey(), 1);
    let (substituted_task, _) = task_pda(&other_creator.pubkey(), 1);
    let profile_before = snapshot_account(&svm, &profile);
    let vault_before = snapshot_account(&svm, &vault_pda(&substituted_task).0);

    let error = send_instruction(
        &mut svm,
        &creator,
        create_task_instruction(
            creator.pubkey(),
            profile,
            substituted_task,
            1,
            DEFAULT_TITLE.to_string(),
            DEFAULT_DESCRIPTION.to_string(),
            DEFAULT_REWARD,
        ),
        &[],
    )
    .unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_unchanged(&svm, &profile, &profile_before);
    assert_account_absent(&svm, &expected_task);
    assert_account_absent(&svm, &substituted_task);
    assert_account_unchanged(&svm, &vault_pda(&substituted_task).0, &vault_before);
}

fn assert_invalid_new_task(
    task_number: u64,
    title: &str,
    description: &str,
    reward: u64,
    assert_error: impl FnOnce(&litesvm::types::FailedTransactionMetadata),
) {
    let (mut svm, creator, profile) = setup_creator();
    let (task, _) = task_pda(&creator.pubkey(), task_number);
    let profile_before = snapshot_account(&svm, &profile);
    let vault_before = snapshot_account(&svm, &vault_pda(&task).0);

    let error = send_instruction(
        &mut svm,
        &creator,
        create_task_instruction(
            creator.pubkey(),
            profile,
            task,
            task_number,
            title.to_string(),
            description.to_string(),
            reward,
        ),
        &[],
    )
    .unwrap_err();

    assert_error(&error);
    assert_failed_creation_rollback(&svm, profile, &profile_before, task, &vault_before);
}
