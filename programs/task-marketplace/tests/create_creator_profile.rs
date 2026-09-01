mod common;

use anchor_lang::prelude::Pubkey;
use solana_signer::Signer;

use common::*;

#[test]
fn successful_creation() {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let profile = create_creator_profile(&mut svm, &creator);

    let account = fetch_creator_profile(&svm, &profile);
    assert_eq!(account.creator, creator.pubkey());
    assert_eq!(account.task_count, 0);
}

#[test]
fn duplicate_creation() {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let profile = create_creator_profile(&mut svm, &creator);
    let profile_before = snapshot_account(&svm, &profile);
    let (task, _) = task_pda(&creator.pubkey(), 1);
    let vault_before = snapshot_account(&svm, &vault_pda(&task).0);

    let error = send_instruction(
        &mut svm,
        &creator,
        create_creator_profile_instruction(creator.pubkey(), profile),
        &[],
    )
    .unwrap_err();

    assert_custom_error(&error, 0);
    assert_account_unchanged(&svm, &profile, &profile_before);
    assert_account_absent(&svm, &task);
    assert_account_unchanged(&svm, &vault_pda(&task).0, &vault_before);
}

#[test]
fn wrong_pda() {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let wrong_profile = Pubkey::new_unique();
    let (expected_profile, _) = creator_profile_pda(&creator.pubkey());
    let (task, _) = task_pda(&creator.pubkey(), 1);
    let vault_before = snapshot_account(&svm, &vault_pda(&task).0);

    let error = send_instruction(
        &mut svm,
        &creator,
        create_creator_profile_instruction(creator.pubkey(), wrong_profile),
        &[],
    )
    .unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_absent(&svm, &expected_profile);
    assert_account_absent(&svm, &wrong_profile);
    assert_account_unchanged(&svm, &vault_pda(&task).0, &vault_before);
}

#[test]
fn missing_signer() {
    let mut svm = bootstrap();
    let payer = funded_keypair(&mut svm);
    let creator = funded_keypair(&mut svm);
    let (profile, _) = creator_profile_pda(&creator.pubkey());
    let (task, _) = task_pda(&creator.pubkey(), 1);
    let vault_before = snapshot_account(&svm, &vault_pda(&task).0);
    let mut instruction = create_creator_profile_instruction(creator.pubkey(), profile);
    instruction.accounts[0].is_signer = false;

    let error = send_instruction(&mut svm, &payer, instruction, &[]).unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::AccountNotSigner);
    assert_account_absent(&svm, &profile);
    assert_account_absent(&svm, &task);
    assert_account_unchanged(&svm, &vault_pda(&task).0, &vault_before);
}
