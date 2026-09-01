mod common;

use anchor_lang::prelude::Pubkey;
use solana_signer::Signer;
use task_marketplace::{
    error::ErrorCode,
    state::{DisputeOutcome, ResolutionState, TaskStatus},
    ARBITRATION_TIMEOUT_SECONDS, TASK_RESOLUTION_VERSION,
};

use common::*;

const SUBMISSION_REFERENCE: &str = "ipfs://submitted-work";
const REJECTION_REFERENCE: &str = "ipfs://creator-rejection-evidence";
const ARBITRATION_FEE: u64 = 50_000;

struct Fixture {
    svm: litesvm::LiteSVM,
    creator: solana_keypair::Keypair,
    worker: solana_keypair::Keypair,
    arbitrator: solana_keypair::Keypair,
    task: Pubkey,
    vault: Pubkey,
    resolution: Pubkey,
}

fn setup_submitted_task() -> Fixture {
    let mut svm = bootstrap();
    set_clock_timestamp(&mut svm, 1_000);
    let creator = funded_keypair(&mut svm);
    let worker = funded_keypair(&mut svm);
    let arbitrator = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    let resolution = initialize_task_resolution(
        &mut svm,
        &creator,
        task,
        arbitrator.pubkey(),
        ARBITRATION_FEE,
    );
    accept_task(&mut svm, &worker, task);
    let vault = fund_task(&mut svm, &creator, task);
    set_clock_timestamp(&mut svm, 1_001);
    submit_task(&mut svm, &worker, task, SUBMISSION_REFERENCE);
    Fixture {
        svm,
        creator,
        worker,
        arbitrator,
        task,
        vault,
        resolution,
    }
}

fn setup_disputed_task() -> Fixture {
    let mut fixture = setup_submitted_task();
    set_clock_timestamp(&mut fixture.svm, 1_002);
    reject_submission(
        &mut fixture.svm,
        &fixture.creator,
        fixture.task,
        fixture.resolution,
        REJECTION_REFERENCE,
    );
    fixture
}

fn assert_dispute_rollback(
    fixture: &Fixture,
    task_before: &Option<AccountSnapshot>,
    resolution_before: &Option<AccountSnapshot>,
    vault_before: &Option<AccountSnapshot>,
    worker_before: u64,
    creator_before: u64,
) {
    assert_account_unchanged(&fixture.svm, &fixture.task, task_before);
    assert_account_unchanged(&fixture.svm, &fixture.resolution, resolution_before);
    assert_account_unchanged(&fixture.svm, &fixture.vault, vault_before);
    assert_eq!(
        balance(&fixture.svm, &fixture.worker.pubkey()),
        worker_before
    );
    assert_eq!(
        balance(&fixture.svm, &fixture.creator.pubkey()),
        creator_before
    );
    assert_vault_solvent(&fixture.svm, &fixture.vault);
}

#[test]
fn initializes_canonical_task_resolution() {
    let fixture = setup_submitted_task();
    let resolution = fetch_task_resolution(&fixture.svm, &fixture.resolution);
    let account = fixture.svm.get_account(&fixture.resolution).unwrap();

    assert_eq!(account.data.len(), 8 + 364);
    assert_eq!(resolution.version, TASK_RESOLUTION_VERSION);
    assert_eq!(resolution.bump, task_resolution_pda(&fixture.task).1);
    assert_eq!(resolution.task, fixture.task);
    assert_eq!(
        resolution.arbitration_authority,
        fixture.arbitrator.pubkey()
    );
    assert_eq!(resolution.arbitration_fee_lamports, ARBITRATION_FEE);
    assert!(resolution.state == ResolutionState::Ready);
    assert_eq!(resolution.opened_at, None);
    assert_eq!(resolution.arbitration_deadline, None);
    assert_eq!(resolution.rejection_reference, None);
    assert_eq!(resolution.outcome, None);
    assert_eq!(resolution.reserved, [0; 64]);
}

#[test]
fn initialization_requires_creator_and_canonical_pda() {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let attacker = funded_keypair(&mut svm);
    let arbitrator = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    let resolution = task_resolution_pda(&task).0;

    let error = send_instruction(
        &mut svm,
        &attacker,
        initialize_task_resolution_instruction(
            attacker.pubkey(),
            task,
            resolution,
            arbitrator.pubkey(),
            0,
        ),
        &[],
    )
    .unwrap_err();
    assert_task_marketplace_error(&error, ErrorCode::Unauthorized);
    assert_account_absent(&svm, &resolution);

    let wrong_resolution = Pubkey::new_unique();
    let error = send_instruction(
        &mut svm,
        &creator,
        initialize_task_resolution_instruction(
            creator.pubkey(),
            task,
            wrong_resolution,
            arbitrator.pubkey(),
            0,
        ),
        &[],
    )
    .unwrap_err();
    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_absent(&svm, &wrong_resolution);
}

#[test]
fn initialization_rejects_invalid_authority_and_non_open_task() {
    for authority_is_creator in [true, false] {
        let mut svm = bootstrap();
        let creator = funded_keypair(&mut svm);
        let worker = funded_keypair(&mut svm);
        create_creator_profile(&mut svm, &creator);
        let task = create_task(&mut svm, &creator, 1);
        let resolution = task_resolution_pda(&task).0;
        let authority = if authority_is_creator {
            creator.pubkey()
        } else {
            Pubkey::default()
        };
        let error = send_instruction(
            &mut svm,
            &creator,
            initialize_task_resolution_instruction(
                creator.pubkey(),
                task,
                resolution,
                authority,
                0,
            ),
            &[],
        )
        .unwrap_err();
        assert_task_marketplace_error(&error, ErrorCode::InvalidArbitrationAuthority);
        assert_account_absent(&svm, &resolution);

        accept_task(&mut svm, &worker, task);
        let error = send_instruction(
            &mut svm,
            &creator,
            initialize_task_resolution_instruction(
                creator.pubkey(),
                task,
                resolution,
                Pubkey::new_unique(),
                0,
            ),
            &[],
        )
        .unwrap_err();
        assert_task_marketplace_error(&error, ErrorCode::InvalidStateTransition);
        assert_account_absent(&svm, &resolution);
    }
}

#[test]
fn creator_rejects_submission_before_review_deadline() {
    let mut fixture = setup_submitted_task();
    let task_before = fetch_task(&fixture.svm, &fixture.task);
    let vault_before = snapshot_account(&fixture.svm, &fixture.vault);
    set_clock_timestamp(&mut fixture.svm, 1_002);

    reject_submission(
        &mut fixture.svm,
        &fixture.creator,
        fixture.task,
        fixture.resolution,
        REJECTION_REFERENCE,
    );

    let task = fetch_task(&fixture.svm, &fixture.task);
    let resolution = fetch_task_resolution(&fixture.svm, &fixture.resolution);
    assert!(task.status == TaskStatus::Disputed);
    assert_eq!(task.worker, task_before.worker);
    assert_eq!(task.submission_reference, task_before.submission_reference);
    assert!(resolution.state == ResolutionState::Disputed);
    assert_eq!(resolution.opened_at, Some(1_002));
    assert_eq!(
        resolution.arbitration_deadline,
        Some(1_002 + ARBITRATION_TIMEOUT_SECONDS)
    );
    assert_eq!(
        resolution.rejection_reference,
        Some(REJECTION_REFERENCE.to_string())
    );
    assert_account_unchanged(&fixture.svm, &fixture.vault, &vault_before);
    assert_vault_solvent(&fixture.svm, &fixture.vault);
}

#[test]
fn rejection_authority_deadline_and_reference_are_enforced() {
    let mut fixture = setup_submitted_task();
    let attacker = funded_keypair(&mut fixture.svm);
    let task_before = snapshot_account(&fixture.svm, &fixture.task);
    let resolution_before = snapshot_account(&fixture.svm, &fixture.resolution);
    let vault_before = snapshot_account(&fixture.svm, &fixture.vault);

    let error = send_instruction(
        &mut fixture.svm,
        &attacker,
        reject_submission_instruction(
            attacker.pubkey(),
            fixture.task,
            fixture.resolution,
            REJECTION_REFERENCE.to_string(),
        ),
        &[],
    )
    .unwrap_err();
    assert_task_marketplace_error(&error, ErrorCode::Unauthorized);
    assert_account_unchanged(&fixture.svm, &fixture.task, &task_before);
    assert_account_unchanged(&fixture.svm, &fixture.resolution, &resolution_before);
    assert_account_unchanged(&fixture.svm, &fixture.vault, &vault_before);

    let deadline = fetch_task(&fixture.svm, &fixture.task)
        .review_deadline
        .unwrap();
    set_clock_timestamp(&mut fixture.svm, deadline);
    let error = send_instruction(
        &mut fixture.svm,
        &fixture.creator,
        reject_submission_instruction(
            fixture.creator.pubkey(),
            fixture.task,
            fixture.resolution,
            REJECTION_REFERENCE.to_string(),
        ),
        &[],
    )
    .unwrap_err();
    assert_task_marketplace_error(&error, ErrorCode::ReviewWindowExpired);

    set_clock_timestamp(&mut fixture.svm, deadline - 1);
    for reference in ["".to_string(), " ".to_string(), "r".repeat(201)] {
        let error = send_instruction(
            &mut fixture.svm,
            &fixture.creator,
            reject_submission_instruction(
                fixture.creator.pubkey(),
                fixture.task,
                fixture.resolution,
                reference,
            ),
            &[],
        )
        .unwrap_err();
        assert_task_marketplace_error(&error, ErrorCode::InvalidRejectionReference);
    }
    assert_account_unchanged(&fixture.svm, &fixture.task, &task_before);
    assert_account_unchanged(&fixture.svm, &fixture.resolution, &resolution_before);
    assert_account_unchanged(&fixture.svm, &fixture.vault, &vault_before);
}

#[test]
fn arbitrator_can_pay_worker_exact_escrow() {
    let mut fixture = setup_disputed_task();
    let worker_before = balance(&fixture.svm, &fixture.worker.pubkey());
    let creator_before = balance(&fixture.svm, &fixture.creator.pubkey());
    let vault_before = balance(&fixture.svm, &fixture.vault);
    let reward = fetch_vault(&fixture.svm, &fixture.vault).escrowed_lamports;

    send_instruction(
        &mut fixture.svm,
        &fixture.arbitrator,
        resolve_dispute_instruction(
            fixture.arbitrator.pubkey(),
            fixture.task,
            fixture.creator.pubkey(),
            fixture.worker.pubkey(),
            fixture.resolution,
            fixture.vault,
            DisputeOutcome::PayWorker,
        ),
        &[],
    )
    .unwrap();

    assert!(fetch_task(&fixture.svm, &fixture.task).status == TaskStatus::Paid);
    let resolution = fetch_task_resolution(&fixture.svm, &fixture.resolution);
    assert!(resolution.state == ResolutionState::Resolved);
    assert_eq!(resolution.outcome, Some(DisputeOutcome::PayWorker));
    assert_eq!(
        balance(&fixture.svm, &fixture.worker.pubkey()),
        worker_before + reward
    );
    assert_eq!(
        balance(&fixture.svm, &fixture.creator.pubkey()),
        creator_before + vault_before - reward
    );
    assert_account_absent(&fixture.svm, &fixture.vault);
}

#[test]
fn arbitrator_can_refund_creator_entire_vault() {
    let mut fixture = setup_disputed_task();
    let worker_before = balance(&fixture.svm, &fixture.worker.pubkey());
    let creator_before = balance(&fixture.svm, &fixture.creator.pubkey());
    let vault_before = balance(&fixture.svm, &fixture.vault);

    send_instruction(
        &mut fixture.svm,
        &fixture.arbitrator,
        resolve_dispute_instruction(
            fixture.arbitrator.pubkey(),
            fixture.task,
            fixture.creator.pubkey(),
            fixture.worker.pubkey(),
            fixture.resolution,
            fixture.vault,
            DisputeOutcome::RefundCreator,
        ),
        &[],
    )
    .unwrap();

    assert!(fetch_task(&fixture.svm, &fixture.task).status == TaskStatus::Refunded);
    assert_eq!(
        fetch_task_resolution(&fixture.svm, &fixture.resolution).outcome,
        Some(DisputeOutcome::RefundCreator)
    );
    assert_eq!(
        balance(&fixture.svm, &fixture.worker.pubkey()),
        worker_before
    );
    assert_eq!(
        balance(&fixture.svm, &fixture.creator.pubkey()),
        creator_before + vault_before
    );
    assert_account_absent(&fixture.svm, &fixture.vault);
}

#[test]
fn wrong_arbitrator_and_expired_arbitration_are_atomic() {
    let mut fixture = setup_disputed_task();
    let attacker = funded_keypair(&mut fixture.svm);
    let task_before = snapshot_account(&fixture.svm, &fixture.task);
    let resolution_before = snapshot_account(&fixture.svm, &fixture.resolution);
    let vault_before = snapshot_account(&fixture.svm, &fixture.vault);
    let worker_before = balance(&fixture.svm, &fixture.worker.pubkey());
    let creator_before = balance(&fixture.svm, &fixture.creator.pubkey());

    let error = send_instruction(
        &mut fixture.svm,
        &attacker,
        resolve_dispute_instruction(
            attacker.pubkey(),
            fixture.task,
            fixture.creator.pubkey(),
            fixture.worker.pubkey(),
            fixture.resolution,
            fixture.vault,
            DisputeOutcome::PayWorker,
        ),
        &[],
    )
    .unwrap_err();
    assert_task_marketplace_error(&error, ErrorCode::Unauthorized);
    assert_dispute_rollback(
        &fixture,
        &task_before,
        &resolution_before,
        &vault_before,
        worker_before,
        creator_before,
    );

    let deadline = fetch_task_resolution(&fixture.svm, &fixture.resolution)
        .arbitration_deadline
        .unwrap();
    set_clock_timestamp(&mut fixture.svm, deadline);
    let error = send_instruction(
        &mut fixture.svm,
        &fixture.arbitrator,
        resolve_dispute_instruction(
            fixture.arbitrator.pubkey(),
            fixture.task,
            fixture.creator.pubkey(),
            fixture.worker.pubkey(),
            fixture.resolution,
            fixture.vault,
            DisputeOutcome::PayWorker,
        ),
        &[],
    )
    .unwrap_err();
    assert_task_marketplace_error(&error, ErrorCode::ArbitrationWindowExpired);
    assert_dispute_rollback(
        &fixture,
        &task_before,
        &resolution_before,
        &vault_before,
        worker_before,
        creator_before,
    );
}

#[test]
fn resolution_metadata_and_parties_cannot_be_substituted() {
    for mutate in [
        (|resolution: &mut task_marketplace::state::TaskResolution| {
            resolution.version = resolution.version.checked_add(1).unwrap();
        }) as fn(&mut task_marketplace::state::TaskResolution),
        (|resolution: &mut task_marketplace::state::TaskResolution| {
            resolution.task = Pubkey::new_unique();
        }) as fn(&mut task_marketplace::state::TaskResolution),
    ] {
        let mut fixture = setup_disputed_task();
        let mut resolution = fetch_task_resolution(&fixture.svm, &fixture.resolution);
        mutate(&mut resolution);
        overwrite_task_resolution(&mut fixture.svm, fixture.resolution, &resolution);
        let task_before = snapshot_account(&fixture.svm, &fixture.task);
        let resolution_before = snapshot_account(&fixture.svm, &fixture.resolution);
        let vault_before = snapshot_account(&fixture.svm, &fixture.vault);
        let error = send_instruction(
            &mut fixture.svm,
            &fixture.arbitrator,
            resolve_dispute_instruction(
                fixture.arbitrator.pubkey(),
                fixture.task,
                fixture.creator.pubkey(),
                fixture.worker.pubkey(),
                fixture.resolution,
                fixture.vault,
                DisputeOutcome::PayWorker,
            ),
            &[],
        )
        .unwrap_err();
        let expected = if resolution.version != TASK_RESOLUTION_VERSION {
            ErrorCode::InvalidResolutionVersion
        } else {
            ErrorCode::InvalidResolutionTask
        };
        assert_task_marketplace_error(&error, expected);
        assert_account_unchanged(&fixture.svm, &fixture.task, &task_before);
        assert_account_unchanged(&fixture.svm, &fixture.resolution, &resolution_before);
        assert_account_unchanged(&fixture.svm, &fixture.vault, &vault_before);
    }

    let mut fixture = setup_disputed_task();
    let wrong = funded_keypair(&mut fixture.svm);
    for (creator, worker) in [
        (wrong.pubkey(), fixture.worker.pubkey()),
        (fixture.creator.pubkey(), wrong.pubkey()),
    ] {
        let error = send_instruction(
            &mut fixture.svm,
            &fixture.arbitrator,
            resolve_dispute_instruction(
                fixture.arbitrator.pubkey(),
                fixture.task,
                creator,
                worker,
                fixture.resolution,
                fixture.vault,
                DisputeOutcome::PayWorker,
            ),
            &[],
        )
        .unwrap_err();
        assert_task_marketplace_error(&error, ErrorCode::Unauthorized);
        assert_vault_solvent(&fixture.svm, &fixture.vault);
    }
}

#[test]
fn arbitrator_signature_is_required() {
    let mut fixture = setup_disputed_task();
    let payer = funded_keypair(&mut fixture.svm);
    let task_before = snapshot_account(&fixture.svm, &fixture.task);
    let resolution_before = snapshot_account(&fixture.svm, &fixture.resolution);
    let vault_before = snapshot_account(&fixture.svm, &fixture.vault);
    let mut instruction = resolve_dispute_instruction(
        fixture.arbitrator.pubkey(),
        fixture.task,
        fixture.creator.pubkey(),
        fixture.worker.pubkey(),
        fixture.resolution,
        fixture.vault,
        DisputeOutcome::PayWorker,
    );
    instruction.accounts[0].is_signer = false;

    let error = send_instruction(&mut fixture.svm, &payer, instruction, &[]).unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::AccountNotSigner);
    assert_account_unchanged(&fixture.svm, &fixture.task, &task_before);
    assert_account_unchanged(&fixture.svm, &fixture.resolution, &resolution_before);
    assert_account_unchanged(&fixture.svm, &fixture.vault, &vault_before);
}

#[test]
fn creator_and_worker_can_resolve_by_agreement() {
    for (outcome, expected_status) in [
        (DisputeOutcome::PayWorker, TaskStatus::Paid),
        (DisputeOutcome::RefundCreator, TaskStatus::Refunded),
    ] {
        let mut fixture = setup_disputed_task();
        let worker_before = balance(&fixture.svm, &fixture.worker.pubkey());
        let reward = fetch_vault(&fixture.svm, &fixture.vault).escrowed_lamports;

        send_instruction(
            &mut fixture.svm,
            &fixture.creator,
            resolve_dispute_by_agreement_instruction(
                fixture.creator.pubkey(),
                fixture.worker.pubkey(),
                fixture.task,
                fixture.resolution,
                fixture.vault,
                outcome,
            ),
            &[&fixture.worker],
        )
        .unwrap();

        assert!(fetch_task(&fixture.svm, &fixture.task).status == expected_status);
        let expected_worker = if outcome == DisputeOutcome::PayWorker {
            worker_before + reward
        } else {
            worker_before
        };
        assert_eq!(
            balance(&fixture.svm, &fixture.worker.pubkey()),
            expected_worker
        );
        assert_account_absent(&fixture.svm, &fixture.vault);
    }
}

#[test]
fn agreement_requires_both_signatures() {
    let mut fixture = setup_disputed_task();
    let task_before = snapshot_account(&fixture.svm, &fixture.task);
    let resolution_before = snapshot_account(&fixture.svm, &fixture.resolution);
    let vault_before = snapshot_account(&fixture.svm, &fixture.vault);
    let mut instruction = resolve_dispute_by_agreement_instruction(
        fixture.creator.pubkey(),
        fixture.worker.pubkey(),
        fixture.task,
        fixture.resolution,
        fixture.vault,
        DisputeOutcome::PayWorker,
    );
    instruction.accounts[1].is_signer = false;

    let error = send_instruction(&mut fixture.svm, &fixture.creator, instruction, &[]).unwrap_err();
    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSigner);
    assert_account_unchanged(&fixture.svm, &fixture.task, &task_before);
    assert_account_unchanged(&fixture.svm, &fixture.resolution, &resolution_before);
    assert_account_unchanged(&fixture.svm, &fixture.vault, &vault_before);
}

#[test]
fn permissionless_timeout_pays_worker_at_boundary() {
    let mut fixture = setup_disputed_task();
    let settler = funded_keypair(&mut fixture.svm);
    let deadline = fetch_task_resolution(&fixture.svm, &fixture.resolution)
        .arbitration_deadline
        .unwrap();
    let worker_before = balance(&fixture.svm, &fixture.worker.pubkey());
    let reward = fetch_vault(&fixture.svm, &fixture.vault).escrowed_lamports;
    set_clock_timestamp(&mut fixture.svm, deadline);

    send_instruction(
        &mut fixture.svm,
        &settler,
        settle_dispute_after_timeout_instruction(
            settler.pubkey(),
            fixture.task,
            fixture.creator.pubkey(),
            fixture.worker.pubkey(),
            fixture.resolution,
            fixture.vault,
        ),
        &[],
    )
    .unwrap();

    assert!(fetch_task(&fixture.svm, &fixture.task).status == TaskStatus::Paid);
    assert_eq!(
        fetch_task_resolution(&fixture.svm, &fixture.resolution).outcome,
        Some(DisputeOutcome::PayWorker)
    );
    assert_eq!(
        balance(&fixture.svm, &fixture.worker.pubkey()),
        worker_before + reward
    );
    assert_account_absent(&fixture.svm, &fixture.vault);
}

#[test]
fn timeout_before_deadline_and_insolvency_are_atomic() {
    let mut fixture = setup_disputed_task();
    let settler = funded_keypair(&mut fixture.svm);
    let deadline = fetch_task_resolution(&fixture.svm, &fixture.resolution)
        .arbitration_deadline
        .unwrap();
    set_clock_timestamp(&mut fixture.svm, deadline - 1);
    let task_before = snapshot_account(&fixture.svm, &fixture.task);
    let resolution_before = snapshot_account(&fixture.svm, &fixture.resolution);
    let vault_before = snapshot_account(&fixture.svm, &fixture.vault);
    let worker_before = balance(&fixture.svm, &fixture.worker.pubkey());
    let creator_before = balance(&fixture.svm, &fixture.creator.pubkey());

    let error = send_instruction(
        &mut fixture.svm,
        &settler,
        settle_dispute_after_timeout_instruction(
            settler.pubkey(),
            fixture.task,
            fixture.creator.pubkey(),
            fixture.worker.pubkey(),
            fixture.resolution,
            fixture.vault,
        ),
        &[],
    )
    .unwrap_err();
    assert_task_marketplace_error(&error, ErrorCode::ArbitrationDeadlineNotReached);
    assert_dispute_rollback(
        &fixture,
        &task_before,
        &resolution_before,
        &vault_before,
        worker_before,
        creator_before,
    );

    let vault_data_len = fixture.svm.get_account(&fixture.vault).unwrap().data.len();
    let rent = fixture
        .svm
        .minimum_balance_for_rent_exemption(vault_data_len);
    let reward = fetch_vault(&fixture.svm, &fixture.vault).escrowed_lamports;
    set_balance(&mut fixture.svm, fixture.vault, rent + reward - 1);
    set_clock_timestamp(&mut fixture.svm, deadline);
    let insolvent_vault_before = snapshot_account(&fixture.svm, &fixture.vault);
    let error = send_instruction(
        &mut fixture.svm,
        &settler,
        settle_dispute_after_timeout_instruction(
            settler.pubkey(),
            fixture.task,
            fixture.creator.pubkey(),
            fixture.worker.pubkey(),
            fixture.resolution,
            fixture.vault,
        ),
        &[],
    )
    .unwrap_err();
    assert_task_marketplace_error(&error, ErrorCode::EscrowBalanceMismatch);
    assert_account_unchanged(&fixture.svm, &fixture.task, &task_before);
    assert_account_unchanged(&fixture.svm, &fixture.resolution, &resolution_before);
    assert_account_unchanged(&fixture.svm, &fixture.vault, &insolvent_vault_before);
}

#[test]
fn ordinary_timeout_settlement_rejects_disputed_resolution() {
    let mut fixture = setup_disputed_task();
    let settler = funded_keypair(&mut fixture.svm);
    let mut task = fetch_task(&fixture.svm, &fixture.task);
    task.status = TaskStatus::Submitted;
    overwrite_task(&mut fixture.svm, fixture.task, &task);
    set_clock_timestamp(&mut fixture.svm, task.review_deadline.unwrap());
    let task_before = snapshot_account(&fixture.svm, &fixture.task);
    let resolution_before = snapshot_account(&fixture.svm, &fixture.resolution);
    let vault_before = snapshot_account(&fixture.svm, &fixture.vault);

    let error = send_instruction(
        &mut fixture.svm,
        &settler,
        settle_task_after_timeout_with_resolution_instruction(
            settler.pubkey(),
            fixture.task,
            fixture.creator.pubkey(),
            fixture.worker.pubkey(),
            fixture.vault,
            fixture.resolution,
        ),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::InvalidResolutionState);
    assert_account_unchanged(&fixture.svm, &fixture.task, &task_before);
    assert_account_unchanged(&fixture.svm, &fixture.resolution, &resolution_before);
    assert_account_unchanged(&fixture.svm, &fixture.vault, &vault_before);
}

#[test]
fn ready_resolution_allows_normal_timeout_settlement() {
    let mut fixture = setup_submitted_task();
    let settler = funded_keypair(&mut fixture.svm);
    let deadline = fetch_task(&fixture.svm, &fixture.task)
        .review_deadline
        .unwrap();
    set_clock_timestamp(&mut fixture.svm, deadline);

    send_instruction(
        &mut fixture.svm,
        &settler,
        settle_task_after_timeout_with_resolution_instruction(
            settler.pubkey(),
            fixture.task,
            fixture.creator.pubkey(),
            fixture.worker.pubkey(),
            fixture.vault,
            fixture.resolution,
        ),
        &[],
    )
    .unwrap();

    assert!(fetch_task(&fixture.svm, &fixture.task).status == TaskStatus::Paid);
    assert!(
        fetch_task_resolution(&fixture.svm, &fixture.resolution).state == ResolutionState::Ready
    );
    assert_account_absent(&fixture.svm, &fixture.vault);
}

#[test]
fn cross_task_resolution_substitution_and_replay_fail() {
    let mut fixture = setup_disputed_task();
    let wrong_resolution = Pubkey::new_unique();
    fixture
        .svm
        .set_account(
            wrong_resolution,
            fixture.svm.get_account(&fixture.resolution).unwrap(),
        )
        .unwrap();
    let error = send_instruction(
        &mut fixture.svm,
        &fixture.arbitrator,
        resolve_dispute_instruction(
            fixture.arbitrator.pubkey(),
            fixture.task,
            fixture.creator.pubkey(),
            fixture.worker.pubkey(),
            wrong_resolution,
            fixture.vault,
            DisputeOutcome::PayWorker,
        ),
        &[],
    )
    .unwrap_err();
    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);

    send_instruction(
        &mut fixture.svm,
        &fixture.arbitrator,
        resolve_dispute_instruction(
            fixture.arbitrator.pubkey(),
            fixture.task,
            fixture.creator.pubkey(),
            fixture.worker.pubkey(),
            fixture.resolution,
            fixture.vault,
            DisputeOutcome::PayWorker,
        ),
        &[],
    )
    .unwrap();
    let task_before = snapshot_account(&fixture.svm, &fixture.task);
    let resolution_before = snapshot_account(&fixture.svm, &fixture.resolution);
    let worker_before = balance(&fixture.svm, &fixture.worker.pubkey());
    let error = send_instruction(
        &mut fixture.svm,
        &fixture.arbitrator,
        resolve_dispute_instruction(
            fixture.arbitrator.pubkey(),
            fixture.task,
            fixture.creator.pubkey(),
            fixture.worker.pubkey(),
            fixture.resolution,
            fixture.vault,
            DisputeOutcome::RefundCreator,
        ),
        &[],
    )
    .unwrap_err();
    assert_framework_error(&error, anchor_lang::error::ErrorCode::AccountNotInitialized);
    assert_account_unchanged(&fixture.svm, &fixture.task, &task_before);
    assert_account_unchanged(&fixture.svm, &fixture.resolution, &resolution_before);
    assert_eq!(
        balance(&fixture.svm, &fixture.worker.pubkey()),
        worker_before
    );
    assert_account_absent(&fixture.svm, &fixture.vault);
}
