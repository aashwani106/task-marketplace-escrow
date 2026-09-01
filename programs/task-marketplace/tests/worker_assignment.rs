mod common;

use anchor_lang::{prelude::Pubkey, Space};
use solana_signer::Signer;
use task_marketplace::{
    error::ErrorCode,
    state::{TaskStatus, WorkerAssignment},
    WORKER_ASSIGNMENT_VERSION,
};

use common::*;

struct Fixture {
    svm: litesvm::LiteSVM,
    creator: solana_keypair::Keypair,
    selected_worker: solana_keypair::Keypair,
    task: Pubkey,
    assignment: Pubkey,
}

fn setup() -> Fixture {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let selected_worker = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    let assignment = worker_assignment_pda(&task).0;

    Fixture {
        svm,
        creator,
        selected_worker,
        task,
        assignment,
    }
}

fn assert_assignment_failure_rollback(
    fixture: &Fixture,
    task_before: &Option<AccountSnapshot>,
    assignment_before: &Option<AccountSnapshot>,
) {
    assert_account_unchanged(&fixture.svm, &fixture.task, task_before);
    assert_account_unchanged(&fixture.svm, &fixture.assignment, assignment_before);
}

#[test]
fn creator_assigns_worker_and_emits_event() {
    let mut fixture = setup();
    let (_, expected_bump) = worker_assignment_pda(&fixture.task);
    let assigned_at_before = fixture
        .svm
        .get_sysvar::<anchor_lang::prelude::Clock>()
        .unix_timestamp;

    let metadata = send_instruction(
        &mut fixture.svm,
        &fixture.creator,
        assign_worker_instruction(
            fixture.creator.pubkey(),
            fixture.task,
            fixture.assignment,
            fixture.selected_worker.pubkey(),
        ),
        &[],
    )
    .unwrap();

    let task = fetch_task(&fixture.svm, &fixture.task);
    let assignment = fetch_worker_assignment(&fixture.svm, &fixture.assignment);
    let assignment_account = fixture.svm.get_account(&fixture.assignment).unwrap();
    assert!(task.status == TaskStatus::Assigned);
    assert_eq!(task.worker, None);
    assert_eq!(assignment.version, WORKER_ASSIGNMENT_VERSION);
    assert_eq!(assignment.bump, expected_bump);
    assert_eq!(assignment.task, fixture.task);
    assert_eq!(assignment.selected_worker, fixture.selected_worker.pubkey());
    assert_eq!(assignment.assigned_at, assigned_at_before);
    assert_eq!(assignment.accepted_at, None);
    assert_eq!(assignment.reserved, [0; 64]);
    assert_eq!(
        assignment_account.data.len(),
        8 + WorkerAssignment::INIT_SPACE
    );
    assert!(metadata
        .logs
        .iter()
        .any(|log| log.starts_with("Program data: ")));
}

#[test]
fn selected_worker_accepts_assignment_and_emits_event() {
    let mut fixture = setup();
    assign_worker(
        &mut fixture.svm,
        &fixture.creator,
        fixture.task,
        fixture.selected_worker.pubkey(),
    );
    let accepted_at = fixture
        .svm
        .get_sysvar::<anchor_lang::prelude::Clock>()
        .unix_timestamp;

    let metadata = send_instruction(
        &mut fixture.svm,
        &fixture.selected_worker,
        accept_assignment_instruction(
            fixture.selected_worker.pubkey(),
            fixture.task,
            fixture.assignment,
        ),
        &[],
    )
    .unwrap();

    let task = fetch_task(&fixture.svm, &fixture.task);
    let assignment = fetch_worker_assignment(&fixture.svm, &fixture.assignment);
    assert!(task.status == TaskStatus::Accepted);
    assert_eq!(task.worker, Some(fixture.selected_worker.pubkey()));
    assert_eq!(assignment.accepted_at, Some(accepted_at));
    assert!(metadata
        .logs
        .iter()
        .any(|log| log.starts_with("Program data: ")));
}

#[test]
fn non_creator_cannot_assign_worker() {
    let mut fixture = setup();
    let attacker = funded_keypair(&mut fixture.svm);
    let task_before = snapshot_account(&fixture.svm, &fixture.task);
    let assignment_before = snapshot_account(&fixture.svm, &fixture.assignment);

    let error = send_instruction(
        &mut fixture.svm,
        &attacker,
        assign_worker_instruction(
            attacker.pubkey(),
            fixture.task,
            fixture.assignment,
            fixture.selected_worker.pubkey(),
        ),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::Unauthorized);
    assert_assignment_failure_rollback(&fixture, &task_before, &assignment_before);
    assert_account_absent(&fixture.svm, &fixture.assignment);
}

#[test]
fn creator_and_default_pubkey_cannot_be_selected() {
    for selected_worker in [Pubkey::default(), Pubkey::new_unique()] {
        let mut fixture = setup();
        let selected_worker = if selected_worker == Pubkey::default() {
            selected_worker
        } else {
            fixture.creator.pubkey()
        };
        let expected = if selected_worker == Pubkey::default() {
            ErrorCode::InvalidSelectedWorker
        } else {
            ErrorCode::Unauthorized
        };
        let task_before = snapshot_account(&fixture.svm, &fixture.task);

        let error = send_instruction(
            &mut fixture.svm,
            &fixture.creator,
            assign_worker_instruction(
                fixture.creator.pubkey(),
                fixture.task,
                fixture.assignment,
                selected_worker,
            ),
            &[],
        )
        .unwrap_err();

        assert_task_marketplace_error(&error, expected);
        assert_account_unchanged(&fixture.svm, &fixture.task, &task_before);
        assert_account_absent(&fixture.svm, &fixture.assignment);
    }
}

#[test]
fn non_open_task_cannot_be_assigned() {
    let mut fixture = setup();
    accept_task(&mut fixture.svm, &fixture.selected_worker, fixture.task);
    let task_before = snapshot_account(&fixture.svm, &fixture.task);

    let error = send_instruction(
        &mut fixture.svm,
        &fixture.creator,
        assign_worker_instruction(
            fixture.creator.pubkey(),
            fixture.task,
            fixture.assignment,
            Pubkey::new_unique(),
        ),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::InvalidStateTransition);
    assert_account_unchanged(&fixture.svm, &fixture.task, &task_before);
    assert_account_absent(&fixture.svm, &fixture.assignment);
}

#[test]
fn wrong_assignment_pda_is_rejected() {
    let mut fixture = setup();
    let wrong_assignment = Pubkey::new_unique();
    let task_before = snapshot_account(&fixture.svm, &fixture.task);

    let error = send_instruction(
        &mut fixture.svm,
        &fixture.creator,
        assign_worker_instruction(
            fixture.creator.pubkey(),
            fixture.task,
            wrong_assignment,
            fixture.selected_worker.pubkey(),
        ),
        &[],
    )
    .unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_unchanged(&fixture.svm, &fixture.task, &task_before);
    assert_account_absent(&fixture.svm, &fixture.assignment);
    assert_account_absent(&fixture.svm, &wrong_assignment);
}

#[test]
fn only_selected_worker_can_accept() {
    let mut fixture = setup();
    let attacker = funded_keypair(&mut fixture.svm);
    assign_worker(
        &mut fixture.svm,
        &fixture.creator,
        fixture.task,
        fixture.selected_worker.pubkey(),
    );
    let task_before = snapshot_account(&fixture.svm, &fixture.task);
    let assignment_before = snapshot_account(&fixture.svm, &fixture.assignment);

    let error = send_instruction(
        &mut fixture.svm,
        &attacker,
        accept_assignment_instruction(attacker.pubkey(), fixture.task, fixture.assignment),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::Unauthorized);
    assert_assignment_failure_rollback(&fixture, &task_before, &assignment_before);
}

#[test]
fn selected_worker_signature_is_required() {
    let mut fixture = setup();
    assign_worker(
        &mut fixture.svm,
        &fixture.creator,
        fixture.task,
        fixture.selected_worker.pubkey(),
    );
    let payer = funded_keypair(&mut fixture.svm);
    let task_before = snapshot_account(&fixture.svm, &fixture.task);
    let assignment_before = snapshot_account(&fixture.svm, &fixture.assignment);
    let mut instruction = accept_assignment_instruction(
        fixture.selected_worker.pubkey(),
        fixture.task,
        fixture.assignment,
    );
    instruction.accounts[0].is_signer = false;

    let error = send_instruction(&mut fixture.svm, &payer, instruction, &[]).unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::AccountNotSigner);
    assert_assignment_failure_rollback(&fixture, &task_before, &assignment_before);
}

#[test]
fn assignment_cannot_be_substituted_across_tasks() {
    let mut fixture = setup();
    let second_task = create_task(&mut fixture.svm, &fixture.creator, 2);
    assign_worker(
        &mut fixture.svm,
        &fixture.creator,
        fixture.task,
        fixture.selected_worker.pubkey(),
    );
    let first_task_before = snapshot_account(&fixture.svm, &fixture.task);
    let second_task_before = snapshot_account(&fixture.svm, &second_task);
    let assignment_before = snapshot_account(&fixture.svm, &fixture.assignment);

    let error = send_instruction(
        &mut fixture.svm,
        &fixture.selected_worker,
        accept_assignment_instruction(
            fixture.selected_worker.pubkey(),
            second_task,
            fixture.assignment,
        ),
        &[],
    )
    .unwrap_err();

    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_unchanged(&fixture.svm, &fixture.task, &first_task_before);
    assert_account_unchanged(&fixture.svm, &second_task, &second_task_before);
    assert_account_unchanged(&fixture.svm, &fixture.assignment, &assignment_before);
}

#[test]
fn corrupted_assignment_metadata_is_rejected() {
    for corrupt_version in [true, false] {
        let mut fixture = setup();
        assign_worker(
            &mut fixture.svm,
            &fixture.creator,
            fixture.task,
            fixture.selected_worker.pubkey(),
        );
        let mut assignment = fetch_worker_assignment(&fixture.svm, &fixture.assignment);
        let expected = if corrupt_version {
            assignment.version = WORKER_ASSIGNMENT_VERSION.saturating_add(1);
            ErrorCode::InvalidAssignmentVersion
        } else {
            assignment.task = Pubkey::new_unique();
            ErrorCode::InvalidAssignmentTask
        };
        overwrite_worker_assignment(&mut fixture.svm, fixture.assignment, &assignment);
        let task_before = snapshot_account(&fixture.svm, &fixture.task);
        let assignment_before = snapshot_account(&fixture.svm, &fixture.assignment);

        let error = send_instruction(
            &mut fixture.svm,
            &fixture.selected_worker,
            accept_assignment_instruction(
                fixture.selected_worker.pubkey(),
                fixture.task,
                fixture.assignment,
            ),
            &[],
        )
        .unwrap_err();

        assert_task_marketplace_error(&error, expected);
        assert_assignment_failure_rollback(&fixture, &task_before, &assignment_before);
    }
}

#[test]
fn replay_acceptance_cannot_mutate_task_or_assignment() {
    let mut fixture = setup();
    assign_worker(
        &mut fixture.svm,
        &fixture.creator,
        fixture.task,
        fixture.selected_worker.pubkey(),
    );
    accept_assignment(
        &mut fixture.svm,
        &fixture.selected_worker,
        fixture.task,
        fixture.assignment,
    );
    let task_before = snapshot_account(&fixture.svm, &fixture.task);
    let assignment_before = snapshot_account(&fixture.svm, &fixture.assignment);

    let error = send_instruction(
        &mut fixture.svm,
        &fixture.selected_worker,
        accept_assignment_instruction(
            fixture.selected_worker.pubkey(),
            fixture.task,
            fixture.assignment,
        ),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::InvalidAssignmentState);
    assert_assignment_failure_rollback(&fixture, &task_before, &assignment_before);
}

#[test]
fn legacy_acceptance_remains_available_for_unassigned_tasks() {
    let mut fixture = setup();

    accept_task(&mut fixture.svm, &fixture.selected_worker, fixture.task);

    let task = fetch_task(&fixture.svm, &fixture.task);
    assert!(task.status == TaskStatus::Accepted);
    assert_eq!(task.worker, Some(fixture.selected_worker.pubkey()));
    assert_account_absent(&fixture.svm, &fixture.assignment);
}

#[test]
fn legacy_acceptance_cannot_bypass_assignment() {
    let mut fixture = setup();
    let attacker = funded_keypair(&mut fixture.svm);
    assign_worker(
        &mut fixture.svm,
        &fixture.creator,
        fixture.task,
        fixture.selected_worker.pubkey(),
    );
    let task_before = snapshot_account(&fixture.svm, &fixture.task);
    let assignment_before = snapshot_account(&fixture.svm, &fixture.assignment);

    let error = send_instruction(
        &mut fixture.svm,
        &attacker,
        accept_task_instruction(attacker.pubkey(), fixture.task),
        &[],
    )
    .unwrap_err();

    assert_task_marketplace_error(&error, ErrorCode::InvalidStateTransition);
    assert_assignment_failure_rollback(&fixture, &task_before, &assignment_before);

    accept_assignment(
        &mut fixture.svm,
        &fixture.selected_worker,
        fixture.task,
        fixture.assignment,
    );
    assert_eq!(
        fetch_task(&fixture.svm, &fixture.task).worker,
        Some(fixture.selected_worker.pubkey())
    );
}

#[test]
fn accepted_assignment_is_compatible_with_funding() {
    let mut fixture = setup();
    assign_worker(
        &mut fixture.svm,
        &fixture.creator,
        fixture.task,
        fixture.selected_worker.pubkey(),
    );
    accept_assignment(
        &mut fixture.svm,
        &fixture.selected_worker,
        fixture.task,
        fixture.assignment,
    );

    let vault = fund_task(&mut fixture.svm, &fixture.creator, fixture.task);

    let task = fetch_task(&fixture.svm, &fixture.task);
    assert!(task.status == TaskStatus::Funded);
    assert_eq!(task.worker, Some(fixture.selected_worker.pubkey()));
    assert_vault_solvent(&fixture.svm, &vault);
}

#[test]
fn creator_can_cancel_an_unaccepted_assignment() {
    let mut fixture = setup();
    assign_worker(
        &mut fixture.svm,
        &fixture.creator,
        fixture.task,
        fixture.selected_worker.pubkey(),
    );
    let assignment_before = snapshot_account(&fixture.svm, &fixture.assignment);

    send_instruction(
        &mut fixture.svm,
        &fixture.creator,
        cancel_task_instruction(fixture.creator.pubkey(), fixture.task),
        &[],
    )
    .unwrap();

    assert!(fetch_task(&fixture.svm, &fixture.task).status == TaskStatus::Cancelled);
    assert_account_unchanged(&fixture.svm, &fixture.assignment, &assignment_before);
}
