mod common;

use solana_signer::Signer;
use task_marketplace::{
    events::{
        AssignmentAccepted, CreatorProfileCreated, DisputeResolved, DisputeResolvedByAgreement,
        DisputeSettledAfterTimeout, SubmissionRejected, TaskAccepted, TaskCancelled, TaskCreated,
        TaskFunded, TaskPaid, TaskRefundedAfterTimeout, TaskResolutionInitialized,
        TaskSettledAfterTimeout, TaskSubmitted, WorkerAssigned,
    },
    state::DisputeOutcome,
    ARBITRATION_TIMEOUT_SECONDS, EVENT_VERSION, REVIEW_TIMEOUT_SECONDS, SUBMISSION_TIMEOUT_SECONDS,
};

use common::*;

const FUND_TIME: i64 = 10_000;
const SUBMIT_TIME: i64 = 10_001;
const REJECT_TIME: i64 = 10_002;
const REJECTION_REFERENCE: &str = "ipfs://event-rejection";

struct TaskFixture {
    svm: litesvm::LiteSVM,
    creator: solana_keypair::Keypair,
    worker: solana_keypair::Keypair,
    task: anchor_lang::prelude::Pubkey,
}

struct ResolutionFixture {
    svm: litesvm::LiteSVM,
    creator: solana_keypair::Keypair,
    worker: solana_keypair::Keypair,
    arbitrator: solana_keypair::Keypair,
    task: anchor_lang::prelude::Pubkey,
    vault: anchor_lang::prelude::Pubkey,
    resolution: anchor_lang::prelude::Pubkey,
}

fn setup_open_task() -> TaskFixture {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let worker = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    TaskFixture {
        svm,
        creator,
        worker,
        task,
    }
}

fn setup_accepted_task() -> TaskFixture {
    let mut fixture = setup_open_task();
    accept_task(&mut fixture.svm, &fixture.worker, fixture.task);
    fixture
}

fn setup_funded_task() -> (TaskFixture, anchor_lang::prelude::Pubkey) {
    let mut fixture = setup_accepted_task();
    set_clock_timestamp(&mut fixture.svm, FUND_TIME);
    let vault = fund_task(&mut fixture.svm, &fixture.creator, fixture.task);
    (fixture, vault)
}

fn setup_submitted_task() -> (TaskFixture, anchor_lang::prelude::Pubkey) {
    let (mut fixture, vault) = setup_funded_task();
    set_clock_timestamp(&mut fixture.svm, SUBMIT_TIME);
    submit_task(
        &mut fixture.svm,
        &fixture.worker,
        fixture.task,
        "ipfs://event-submission",
    );
    (fixture, vault)
}

fn setup_submitted_resolution() -> ResolutionFixture {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let worker = funded_keypair(&mut svm);
    let arbitrator = funded_keypair(&mut svm);
    create_creator_profile(&mut svm, &creator);
    let task = create_task(&mut svm, &creator, 1);
    let resolution = initialize_task_resolution(&mut svm, &creator, task, arbitrator.pubkey(), 99);
    accept_task(&mut svm, &worker, task);
    set_clock_timestamp(&mut svm, FUND_TIME);
    let vault = fund_task(&mut svm, &creator, task);
    set_clock_timestamp(&mut svm, SUBMIT_TIME);
    submit_task(&mut svm, &worker, task, "ipfs://event-submission");
    ResolutionFixture {
        svm,
        creator,
        worker,
        arbitrator,
        task,
        vault,
        resolution,
    }
}

fn setup_disputed_task() -> ResolutionFixture {
    let mut fixture = setup_submitted_resolution();
    set_clock_timestamp(&mut fixture.svm, REJECT_TIME);
    reject_submission(
        &mut fixture.svm,
        &fixture.creator,
        fixture.task,
        fixture.resolution,
        REJECTION_REFERENCE,
    );
    fixture
}

#[test]
fn creation_event_payloads_are_exact() {
    let mut svm = bootstrap();
    let creator = funded_keypair(&mut svm);
    let profile = creator_profile_pda(&creator.pubkey()).0;
    set_clock_timestamp(&mut svm, 100);

    let metadata = send_instruction(
        &mut svm,
        &creator,
        create_creator_profile_instruction(creator.pubkey(), profile),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &CreatorProfileCreated {
            version: EVENT_VERSION,
            creator_profile: profile,
            creator: creator.pubkey(),
            actor: creator.pubkey(),
            created_at: 100,
        },
    );

    let task = task_pda(&creator.pubkey(), 1).0;
    set_clock_timestamp(&mut svm, 101);
    let metadata = send_instruction(
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
    .unwrap();
    assert_event(
        &metadata,
        &TaskCreated {
            version: EVENT_VERSION,
            task,
            creator: creator.pubkey(),
            actor: creator.pubkey(),
            created_at: 101,
            reward_amount: DEFAULT_REWARD,
        },
    );
}

#[test]
fn assignment_event_payloads_are_exact() {
    let mut fixture = setup_open_task();
    let assignment = worker_assignment_pda(&fixture.task).0;
    set_clock_timestamp(&mut fixture.svm, 200);

    let metadata = send_instruction(
        &mut fixture.svm,
        &fixture.creator,
        assign_worker_instruction(
            fixture.creator.pubkey(),
            fixture.task,
            assignment,
            fixture.worker.pubkey(),
        ),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &WorkerAssigned {
            version: EVENT_VERSION,
            task: fixture.task,
            creator: fixture.creator.pubkey(),
            worker: fixture.worker.pubkey(),
            actor: fixture.creator.pubkey(),
            assigned_at: 200,
        },
    );

    set_clock_timestamp(&mut fixture.svm, 201);
    let metadata = send_instruction(
        &mut fixture.svm,
        &fixture.worker,
        accept_assignment_instruction(fixture.worker.pubkey(), fixture.task, assignment),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &AssignmentAccepted {
            version: EVENT_VERSION,
            task: fixture.task,
            creator: fixture.creator.pubkey(),
            worker: fixture.worker.pubkey(),
            actor: fixture.worker.pubkey(),
            accepted_at: 201,
        },
    );
}

#[test]
fn acceptance_event_payload_is_exact() {
    let mut fixture = setup_open_task();
    set_clock_timestamp(&mut fixture.svm, 300);

    let metadata = send_instruction(
        &mut fixture.svm,
        &fixture.worker,
        accept_task_instruction(fixture.worker.pubkey(), fixture.task),
        &[],
    )
    .unwrap();

    assert_event(
        &metadata,
        &TaskAccepted {
            version: EVENT_VERSION,
            task: fixture.task,
            creator: fixture.creator.pubkey(),
            worker: fixture.worker.pubkey(),
            actor: fixture.worker.pubkey(),
            accepted_at: 300,
        },
    );
}

#[test]
fn funding_and_submission_event_payloads_are_exact() {
    let mut fixture = setup_accepted_task();
    let vault = vault_pda(&fixture.task).0;
    set_clock_timestamp(&mut fixture.svm, FUND_TIME);

    let metadata = send_instruction(
        &mut fixture.svm,
        &fixture.creator,
        fund_task_instruction(fixture.creator.pubkey(), fixture.task, vault),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &TaskFunded {
            version: EVENT_VERSION,
            task: fixture.task,
            creator: fixture.creator.pubkey(),
            worker: fixture.worker.pubkey(),
            actor: fixture.creator.pubkey(),
            funded_at: FUND_TIME,
            submission_deadline: FUND_TIME + SUBMISSION_TIMEOUT_SECONDS,
            reward_amount: DEFAULT_REWARD,
        },
    );

    set_clock_timestamp(&mut fixture.svm, SUBMIT_TIME);
    let metadata = send_instruction(
        &mut fixture.svm,
        &fixture.worker,
        submit_task_instruction(
            fixture.worker.pubkey(),
            fixture.task,
            "ipfs://event-submission".to_string(),
        ),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &TaskSubmitted {
            version: EVENT_VERSION,
            task: fixture.task,
            creator: fixture.creator.pubkey(),
            worker: fixture.worker.pubkey(),
            actor: fixture.worker.pubkey(),
            submitted_at: SUBMIT_TIME,
            review_deadline: SUBMIT_TIME + REVIEW_TIMEOUT_SECONDS,
        },
    );
}

#[test]
fn cancellation_and_creator_payment_event_payloads_are_exact() {
    let mut cancelled = setup_accepted_task();
    set_clock_timestamp(&mut cancelled.svm, 400);
    let metadata = send_instruction(
        &mut cancelled.svm,
        &cancelled.creator,
        cancel_task_instruction(cancelled.creator.pubkey(), cancelled.task),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &TaskCancelled {
            version: EVENT_VERSION,
            task: cancelled.task,
            creator: cancelled.creator.pubkey(),
            worker: Some(cancelled.worker.pubkey()),
            actor: cancelled.creator.pubkey(),
            cancelled_at: 400,
        },
    );

    let (mut paid, vault) = setup_submitted_task();
    set_clock_timestamp(&mut paid.svm, 10_003);
    let metadata = send_instruction(
        &mut paid.svm,
        &paid.creator,
        pay_task_instruction(
            paid.creator.pubkey(),
            paid.task,
            vault,
            paid.worker.pubkey(),
        ),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &TaskPaid {
            version: EVENT_VERSION,
            task: paid.task,
            creator: paid.creator.pubkey(),
            worker: paid.worker.pubkey(),
            actor: paid.creator.pubkey(),
            paid_at: 10_003,
            reward_amount: DEFAULT_REWARD,
        },
    );
}

#[test]
fn timeout_recovery_event_payloads_are_exact() {
    let (mut refunded, vault) = setup_funded_task();
    let submission_deadline = FUND_TIME + SUBMISSION_TIMEOUT_SECONDS;
    set_clock_timestamp(&mut refunded.svm, submission_deadline);
    let metadata = send_instruction(
        &mut refunded.svm,
        &refunded.creator,
        refund_task_after_timeout_instruction(refunded.creator.pubkey(), refunded.task, vault),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &TaskRefundedAfterTimeout {
            version: EVENT_VERSION,
            task: refunded.task,
            creator: refunded.creator.pubkey(),
            worker: refunded.worker.pubkey(),
            actor: refunded.creator.pubkey(),
            refunded_at: submission_deadline,
            submission_deadline,
            reward_amount: DEFAULT_REWARD,
        },
    );

    let (mut settled, vault) = setup_submitted_task();
    let settler = funded_keypair(&mut settled.svm);
    let review_deadline = SUBMIT_TIME + REVIEW_TIMEOUT_SECONDS;
    set_clock_timestamp(&mut settled.svm, review_deadline);
    let metadata = send_instruction(
        &mut settled.svm,
        &settler,
        settle_task_after_timeout_instruction(
            settler.pubkey(),
            settled.task,
            settled.creator.pubkey(),
            settled.worker.pubkey(),
            vault,
        ),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &TaskSettledAfterTimeout {
            version: EVENT_VERSION,
            task: settled.task,
            creator: settled.creator.pubkey(),
            worker: settled.worker.pubkey(),
            actor: settler.pubkey(),
            settled_at: review_deadline,
            review_deadline,
            reward_amount: DEFAULT_REWARD,
        },
    );
}

#[test]
fn resolution_initialization_and_rejection_event_payloads_are_exact() {
    let mut fixture = setup_open_task();
    let arbitrator = funded_keypair(&mut fixture.svm);
    let resolution = task_resolution_pda(&fixture.task).0;
    set_clock_timestamp(&mut fixture.svm, 500);

    let metadata = send_instruction(
        &mut fixture.svm,
        &fixture.creator,
        initialize_task_resolution_instruction(
            fixture.creator.pubkey(),
            fixture.task,
            resolution,
            arbitrator.pubkey(),
            99,
        ),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &TaskResolutionInitialized {
            version: EVENT_VERSION,
            task: fixture.task,
            creator: fixture.creator.pubkey(),
            actor: fixture.creator.pubkey(),
            arbitration_authority: arbitrator.pubkey(),
            arbitration_fee_lamports: 99,
            initialized_at: 500,
        },
    );

    accept_task(&mut fixture.svm, &fixture.worker, fixture.task);
    set_clock_timestamp(&mut fixture.svm, FUND_TIME);
    fund_task(&mut fixture.svm, &fixture.creator, fixture.task);
    set_clock_timestamp(&mut fixture.svm, SUBMIT_TIME);
    submit_task(
        &mut fixture.svm,
        &fixture.worker,
        fixture.task,
        "ipfs://event-submission",
    );
    set_clock_timestamp(&mut fixture.svm, REJECT_TIME);
    let metadata = send_instruction(
        &mut fixture.svm,
        &fixture.creator,
        reject_submission_instruction(
            fixture.creator.pubkey(),
            fixture.task,
            resolution,
            REJECTION_REFERENCE.to_string(),
        ),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &SubmissionRejected {
            version: EVENT_VERSION,
            task: fixture.task,
            creator: fixture.creator.pubkey(),
            worker: fixture.worker.pubkey(),
            actor: fixture.creator.pubkey(),
            rejected_at: REJECT_TIME,
            arbitration_deadline: REJECT_TIME + ARBITRATION_TIMEOUT_SECONDS,
        },
    );
}

#[test]
fn dispute_resolution_event_payloads_are_exact() {
    let mut arbitrated = setup_disputed_task();
    set_clock_timestamp(&mut arbitrated.svm, REJECT_TIME + 1);
    let metadata = send_instruction(
        &mut arbitrated.svm,
        &arbitrated.arbitrator,
        resolve_dispute_instruction(
            arbitrated.arbitrator.pubkey(),
            arbitrated.task,
            arbitrated.creator.pubkey(),
            arbitrated.worker.pubkey(),
            arbitrated.resolution,
            arbitrated.vault,
            DisputeOutcome::PayWorker,
        ),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &DisputeResolved {
            version: EVENT_VERSION,
            task: arbitrated.task,
            creator: arbitrated.creator.pubkey(),
            worker: arbitrated.worker.pubkey(),
            actor: arbitrated.arbitrator.pubkey(),
            resolved_at: REJECT_TIME + 1,
            reward_amount: DEFAULT_REWARD,
            outcome: DisputeOutcome::PayWorker,
        },
    );

    let mut agreed = setup_disputed_task();
    set_clock_timestamp(&mut agreed.svm, REJECT_TIME + 2);
    let metadata = send_instruction(
        &mut agreed.svm,
        &agreed.creator,
        resolve_dispute_by_agreement_instruction(
            agreed.creator.pubkey(),
            agreed.worker.pubkey(),
            agreed.task,
            agreed.resolution,
            agreed.vault,
            DisputeOutcome::RefundCreator,
        ),
        &[&agreed.worker],
    )
    .unwrap();
    assert_event(
        &metadata,
        &DisputeResolvedByAgreement {
            version: EVENT_VERSION,
            task: agreed.task,
            creator: agreed.creator.pubkey(),
            worker: agreed.worker.pubkey(),
            actor: agreed.creator.pubkey(),
            resolved_at: REJECT_TIME + 2,
            reward_amount: DEFAULT_REWARD,
            outcome: DisputeOutcome::RefundCreator,
        },
    );

    let mut timed_out = setup_disputed_task();
    let settler = funded_keypair(&mut timed_out.svm);
    let arbitration_deadline = REJECT_TIME + ARBITRATION_TIMEOUT_SECONDS;
    set_clock_timestamp(&mut timed_out.svm, arbitration_deadline);
    let metadata = send_instruction(
        &mut timed_out.svm,
        &settler,
        settle_dispute_after_timeout_instruction(
            settler.pubkey(),
            timed_out.task,
            timed_out.creator.pubkey(),
            timed_out.worker.pubkey(),
            timed_out.resolution,
            timed_out.vault,
        ),
        &[],
    )
    .unwrap();
    assert_event(
        &metadata,
        &DisputeSettledAfterTimeout {
            version: EVENT_VERSION,
            task: timed_out.task,
            creator: timed_out.creator.pubkey(),
            worker: timed_out.worker.pubkey(),
            actor: settler.pubkey(),
            settled_at: arbitration_deadline,
            reward_amount: DEFAULT_REWARD,
            outcome: DisputeOutcome::PayWorker,
        },
    );
}
