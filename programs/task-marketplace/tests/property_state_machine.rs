mod common;

use anchor_lang::{prelude::Pubkey, AnchorSerialize};
use proptest::{prelude::*, test_runner::TestCaseResult};
use solana_keypair::Keypair;
use solana_signer::Signer;
use task_marketplace::{
    state::{DisputeOutcome, ResolutionState, Task, TaskResolution, TaskStatus, WorkerAssignment},
    TASK_RESOLUTION_VERSION, WORKER_ASSIGNMENT_VERSION,
};

use common::*;

const INITIAL_TIME: i64 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Party {
    Creator,
    WorkerA,
    WorkerB,
    Arbitrator,
    Outsider,
}

#[derive(Clone, Copy, Debug)]
enum Action {
    Assign(Party),
    AcceptAssignment(Party),
    AcceptTask(Party),
    InitializeResolution(Party),
    Fund(Party),
    Submit(Party),
    Cancel(Party),
    Pay(Party),
    RefundAfterTimeout(Party),
    Reject(Party),
    Resolve(Party, DisputeOutcome),
    ResolveByAgreement(DisputeOutcome),
    SettleTaskAfterTimeout,
    SettleDisputeAfterTimeout,
    AdvanceOneSecond,
    AdvanceToSubmissionDeadline,
    AdvanceToReviewDeadline,
    AdvanceToArbitrationDeadline,
}

fn party_strategy() -> impl Strategy<Value = Party> {
    prop_oneof![
        Just(Party::Creator),
        Just(Party::WorkerA),
        Just(Party::WorkerB),
        Just(Party::Arbitrator),
        Just(Party::Outsider),
    ]
}

fn outcome_strategy() -> impl Strategy<Value = DisputeOutcome> {
    prop_oneof![
        Just(DisputeOutcome::PayWorker),
        Just(DisputeOutcome::RefundCreator),
    ]
}

fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        party_strategy().prop_map(Action::Assign),
        party_strategy().prop_map(Action::AcceptAssignment),
        party_strategy().prop_map(Action::AcceptTask),
        party_strategy().prop_map(Action::InitializeResolution),
        party_strategy().prop_map(Action::Fund),
        party_strategy().prop_map(Action::Submit),
        party_strategy().prop_map(Action::Cancel),
        party_strategy().prop_map(Action::Pay),
        party_strategy().prop_map(Action::RefundAfterTimeout),
        party_strategy().prop_map(Action::Reject),
        (party_strategy(), outcome_strategy())
            .prop_map(|(party, outcome)| Action::Resolve(party, outcome)),
        outcome_strategy().prop_map(Action::ResolveByAgreement),
        Just(Action::SettleTaskAfterTimeout),
        Just(Action::SettleDisputeAfterTimeout),
        Just(Action::AdvanceOneSecond),
        Just(Action::AdvanceToSubmissionDeadline),
        Just(Action::AdvanceToReviewDeadline),
        Just(Action::AdvanceToArbitrationDeadline),
    ]
}

fn sequence_strategy(max_random_tail: usize) -> impl Strategy<Value = Vec<Action>> {
    let valid_prefix = prop_oneof![
        Just(vec![
            Action::AcceptTask(Party::WorkerA),
            Action::Fund(Party::Creator),
            Action::Submit(Party::WorkerA),
            Action::Pay(Party::Creator),
        ]),
        Just(vec![
            Action::Assign(Party::Creator),
            Action::AcceptAssignment(Party::WorkerA),
            Action::Fund(Party::Creator),
            Action::Submit(Party::WorkerA),
            Action::Pay(Party::Creator),
        ]),
        Just(vec![
            Action::AcceptTask(Party::WorkerA),
            Action::Fund(Party::Creator),
            Action::AdvanceToSubmissionDeadline,
            Action::RefundAfterTimeout(Party::Creator),
        ]),
        Just(vec![
            Action::AcceptTask(Party::WorkerA),
            Action::Fund(Party::Creator),
            Action::Submit(Party::WorkerA),
            Action::AdvanceToReviewDeadline,
            Action::SettleTaskAfterTimeout,
        ]),
        Just(vec![
            Action::InitializeResolution(Party::Creator),
            Action::AcceptTask(Party::WorkerA),
            Action::Fund(Party::Creator),
            Action::Submit(Party::WorkerA),
            Action::Reject(Party::Creator),
            Action::Resolve(Party::Arbitrator, DisputeOutcome::PayWorker),
        ]),
        Just(vec![
            Action::InitializeResolution(Party::Creator),
            Action::AcceptTask(Party::WorkerA),
            Action::Fund(Party::Creator),
            Action::Submit(Party::WorkerA),
            Action::Reject(Party::Creator),
            Action::AdvanceToArbitrationDeadline,
            Action::SettleDisputeAfterTimeout,
        ]),
        Just(vec![
            Action::InitializeResolution(Party::Creator),
            Action::AcceptTask(Party::WorkerA),
            Action::Fund(Party::Creator),
            Action::Submit(Party::WorkerA),
            Action::Reject(Party::Creator),
            Action::ResolveByAgreement(DisputeOutcome::RefundCreator),
        ]),
        Just(vec![Action::Cancel(Party::Creator)]),
        prop::collection::vec(action_strategy(), 1..8),
    ];

    (
        valid_prefix,
        prop::collection::vec(action_strategy(), 0..max_random_tail),
    )
        .prop_map(|(mut prefix, tail)| {
            prefix.extend(tail);
            prefix
        })
}

fn serialize<T: AnchorSerialize>(value: &T) -> Vec<u8> {
    let mut bytes = Vec::new();
    value.serialize(&mut bytes).unwrap();
    bytes
}

fn is_terminal(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Paid | TaskStatus::Refunded | TaskStatus::Cancelled
    )
}

struct Model {
    task_key: Pubkey,
    creator: Pubkey,
    worker_a: Pubkey,
    worker_b: Pubkey,
    arbitrator: Pubkey,
    outsider: Pubkey,
    task: Task,
    assignment: Option<WorkerAssignment>,
    resolution: Option<TaskResolution>,
    vault_liability: Option<u64>,
    now: i64,
    worker_received: u64,
    payout_count: u8,
    refund_count: u8,
    vault_close_count: u8,
    dispute_finish_count: u8,
}

impl Model {
    fn new() -> Self {
        let creator = Pubkey::new_unique();
        Self {
            task_key: Pubkey::new_unique(),
            creator,
            worker_a: Pubkey::new_unique(),
            worker_b: Pubkey::new_unique(),
            arbitrator: Pubkey::new_unique(),
            outsider: Pubkey::new_unique(),
            task: Task {
                task_number: 1,
                creator,
                worker: None,
                title: "Property task".to_string(),
                description: "Property state-machine task".to_string(),
                reward_amount: DEFAULT_REWARD,
                status: TaskStatus::Open,
                submission_reference: None,
                funded_at: None,
                submission_deadline: None,
                review_deadline: None,
            },
            assignment: None,
            resolution: None,
            vault_liability: None,
            now: INITIAL_TIME,
            worker_received: 0,
            payout_count: 0,
            refund_count: 0,
            vault_close_count: 0,
            dispute_finish_count: 0,
        }
    }

    fn key(&self, party: Party) -> Pubkey {
        match party {
            Party::Creator => self.creator,
            Party::WorkerA => self.worker_a,
            Party::WorkerB => self.worker_b,
            Party::Arbitrator => self.arbitrator,
            Party::Outsider => self.outsider,
        }
    }

    fn close_vault(&mut self, outcome: DisputeOutcome) {
        let liability = self.vault_liability.take().unwrap();
        self.vault_close_count += 1;
        match outcome {
            DisputeOutcome::PayWorker => {
                self.worker_received = self.worker_received.checked_add(liability).unwrap();
                self.payout_count += 1;
            }
            DisputeOutcome::RefundCreator => self.refund_count += 1,
        }
    }

    fn apply(&mut self, action: Action) -> bool {
        match action {
            Action::Assign(party) => {
                if party != Party::Creator || self.assignment.is_some() {
                    return false;
                }
                let selected_worker = self.worker_a;
                if self.task.assign_worker(selected_worker).is_err() {
                    return false;
                }
                self.assignment = Some(WorkerAssignment {
                    version: WORKER_ASSIGNMENT_VERSION,
                    bump: 255,
                    task: self.task_key,
                    selected_worker,
                    assigned_at: self.now,
                    accepted_at: None,
                    reserved: [0; 64],
                });
                true
            }
            Action::AcceptAssignment(party) => {
                let worker = self.key(party);
                let Some(assignment) = self.assignment.as_mut() else {
                    return false;
                };
                if assignment.selected_worker != worker
                    || assignment.accepted_at.is_some()
                    || self.task.status != TaskStatus::Assigned
                    || self.task.worker.is_some()
                {
                    return false;
                }
                assignment.accept(worker, self.now).unwrap();
                self.task.accept_assignment(worker).unwrap();
                true
            }
            Action::AcceptTask(party) => self.task.accept(self.key(party)).is_ok(),
            Action::InitializeResolution(party) => {
                if party != Party::Creator
                    || self.resolution.is_some()
                    || self.task.status != TaskStatus::Open
                {
                    return false;
                }
                self.resolution = Some(TaskResolution {
                    version: TASK_RESOLUTION_VERSION,
                    bump: 255,
                    task: self.task_key,
                    arbitration_authority: self.arbitrator,
                    arbitration_fee_lamports: 0,
                    state: ResolutionState::Ready,
                    opened_at: None,
                    arbitration_deadline: None,
                    rejection_reference: None,
                    outcome: None,
                    reserved: [0; 64],
                });
                true
            }
            Action::Fund(party) => {
                if party != Party::Creator || self.vault_liability.is_some() {
                    return false;
                }
                if self.task.fund(self.now).is_err() {
                    return false;
                }
                self.vault_liability = Some(self.task.reward_amount);
                true
            }
            Action::Submit(party) => self
                .task
                .submit(
                    self.key(party),
                    "ipfs://property-submission".to_string(),
                    self.now,
                )
                .is_ok(),
            Action::Cancel(party) => party == Party::Creator && self.task.cancel().is_ok(),
            Action::Pay(party) => {
                if party != Party::Creator || self.vault_liability.is_none() {
                    return false;
                }
                if self.task.pay().is_err() {
                    return false;
                }
                self.close_vault(DisputeOutcome::PayWorker);
                true
            }
            Action::RefundAfterTimeout(party) => {
                if party != Party::Creator || self.vault_liability.is_none() {
                    return false;
                }
                if self.task.refund_after_timeout(self.now).is_err() {
                    return false;
                }
                self.close_vault(DisputeOutcome::RefundCreator);
                true
            }
            Action::Reject(party) => {
                if party != Party::Creator {
                    return false;
                }
                let Some(resolution) = self.resolution.as_mut() else {
                    return false;
                };
                if resolution.state != ResolutionState::Ready
                    || self.task.status != TaskStatus::Submitted
                {
                    return false;
                }
                self.task.reject_submission(self.now).unwrap();
                resolution
                    .open_dispute(self.now, "ipfs://property-rejection".to_string())
                    .unwrap();
                true
            }
            Action::Resolve(party, outcome) => {
                if party != Party::Arbitrator || self.vault_liability.is_none() {
                    return false;
                }
                let Some(resolution) = self.resolution.as_mut() else {
                    return false;
                };
                if resolution.resolve(outcome, self.now).is_err() {
                    return false;
                }
                self.task.resolve_dispute(outcome).unwrap();
                self.dispute_finish_count += 1;
                self.close_vault(outcome);
                true
            }
            Action::ResolveByAgreement(outcome) => {
                if self.vault_liability.is_none() {
                    return false;
                }
                let Some(resolution) = self.resolution.as_mut() else {
                    return false;
                };
                if resolution.resolve_by_agreement(outcome).is_err() {
                    return false;
                }
                self.task.resolve_dispute(outcome).unwrap();
                self.dispute_finish_count += 1;
                self.close_vault(outcome);
                true
            }
            Action::SettleTaskAfterTimeout => {
                if self.vault_liability.is_none()
                    || self
                        .resolution
                        .as_ref()
                        .is_some_and(|resolution| resolution.state != ResolutionState::Ready)
                {
                    return false;
                }
                if self.task.pay_after_timeout(self.now).is_err() {
                    return false;
                }
                self.close_vault(DisputeOutcome::PayWorker);
                true
            }
            Action::SettleDisputeAfterTimeout => {
                if self.vault_liability.is_none() {
                    return false;
                }
                let Some(resolution) = self.resolution.as_mut() else {
                    return false;
                };
                if resolution.settle_after_timeout(self.now).is_err() {
                    return false;
                }
                self.task
                    .resolve_dispute(DisputeOutcome::PayWorker)
                    .unwrap();
                self.dispute_finish_count += 1;
                self.close_vault(DisputeOutcome::PayWorker);
                true
            }
            Action::AdvanceOneSecond => {
                self.now = self.now.checked_add(1).unwrap();
                false
            }
            Action::AdvanceToSubmissionDeadline => {
                if let Some(deadline) = self.task.submission_deadline {
                    self.now = deadline;
                }
                false
            }
            Action::AdvanceToReviewDeadline => {
                if let Some(deadline) = self.task.review_deadline {
                    self.now = deadline;
                }
                false
            }
            Action::AdvanceToArbitrationDeadline => {
                if let Some(deadline) = self
                    .resolution
                    .as_ref()
                    .and_then(|resolution| resolution.arbitration_deadline)
                {
                    self.now = deadline;
                }
                false
            }
        }
    }

    fn assert_invariants(&self) -> TestCaseResult {
        match self.task.status {
            TaskStatus::Open | TaskStatus::Assigned => prop_assert!(self.task.worker.is_none()),
            TaskStatus::Accepted => prop_assert!(self.task.worker.is_some()),
            TaskStatus::Funded => prop_assert!(self.task.funded_at.is_some()),
            TaskStatus::Submitted => prop_assert!(self.task.submission_reference.is_some()),
            TaskStatus::Disputed => {
                let resolution_is_disputed = self
                    .resolution
                    .as_ref()
                    .is_some_and(|resolution| resolution.state == ResolutionState::Disputed);
                prop_assert!(resolution_is_disputed);
            }
            TaskStatus::Paid | TaskStatus::Cancelled | TaskStatus::Refunded => {}
        }
        if let Some(liability) = self.vault_liability {
            prop_assert_eq!(liability, self.task.reward_amount);
        }
        prop_assert!(self.worker_received <= self.task.reward_amount);
        prop_assert!(self.payout_count <= 1);
        prop_assert!(self.refund_count <= 1);
        prop_assert!(self.vault_close_count <= 1);
        prop_assert!(self.dispute_finish_count <= 1);
        Ok(())
    }
}

fn check_model_sequence(actions: &[Action]) -> TestCaseResult {
    let mut model = Model::new();
    model.assert_invariants()?;

    for &action in actions {
        let previous_status = model.task.status;
        let task_before = serialize(&model.task);
        let was_disputed = previous_status == TaskStatus::Disputed;
        let success = model.apply(action);

        if is_terminal(previous_status) {
            prop_assert_eq!(serialize(&model.task), task_before);
            prop_assert!(!success);
        }
        if was_disputed && matches!(action, Action::Pay(_) | Action::SettleTaskAfterTimeout) {
            prop_assert!(!success);
        }
        if success {
            if let Action::AcceptAssignment(party) = action {
                prop_assert_eq!(party, Party::WorkerA);
            }
        }
        model.assert_invariants()?;

        if success && is_terminal(model.task.status) {
            let terminal_task = serialize(&model.task);
            let payout_count = model.payout_count;
            let refund_count = model.refund_count;
            let close_count = model.vault_close_count;
            prop_assert!(!model.apply(action));
            prop_assert_eq!(serialize(&model.task), terminal_task);
            prop_assert_eq!(model.payout_count, payout_count);
            prop_assert_eq!(model.refund_count, refund_count);
            prop_assert_eq!(model.vault_close_count, close_count);
        }
    }

    Ok(())
}

struct OnchainHarness {
    svm: litesvm::LiteSVM,
    creator: Keypair,
    worker_a: Keypair,
    worker_b: Keypair,
    arbitrator: Keypair,
    outsider: Keypair,
    fee_payer: Keypair,
    task: Pubkey,
    assignment: Pubkey,
    resolution: Pubkey,
    vault: Pubkey,
    now: i64,
    initial_worker_balance: u64,
}

impl OnchainHarness {
    fn new() -> Self {
        let mut svm = bootstrap();
        let creator = funded_keypair(&mut svm);
        let worker_a = funded_keypair(&mut svm);
        let worker_b = funded_keypair(&mut svm);
        let arbitrator = funded_keypair(&mut svm);
        let outsider = funded_keypair(&mut svm);
        let fee_payer = funded_keypair(&mut svm);
        create_creator_profile(&mut svm, &creator);
        let task = create_task(&mut svm, &creator, 1);
        let assignment = worker_assignment_pda(&task).0;
        let resolution = task_resolution_pda(&task).0;
        let vault = vault_pda(&task).0;
        set_clock_timestamp(&mut svm, INITIAL_TIME);
        let initial_worker_balance = balance(&svm, &worker_a.pubkey())
            .checked_add(balance(&svm, &worker_b.pubkey()))
            .unwrap();
        Self {
            svm,
            creator,
            worker_a,
            worker_b,
            arbitrator,
            outsider,
            fee_payer,
            task,
            assignment,
            resolution,
            vault,
            now: INITIAL_TIME,
            initial_worker_balance,
        }
    }

    fn key(&self, party: Party) -> Pubkey {
        match party {
            Party::Creator => self.creator.pubkey(),
            Party::WorkerA => self.worker_a.pubkey(),
            Party::WorkerB => self.worker_b.pubkey(),
            Party::Arbitrator => self.arbitrator.pubkey(),
            Party::Outsider => self.outsider.pubkey(),
        }
    }

    fn stored_worker_or_default(&self) -> Pubkey {
        fetch_task(&self.svm, &self.task)
            .worker
            .unwrap_or(self.worker_a.pubkey())
    }

    fn send_as(&mut self, party: Party, instruction: solana_transaction::Instruction) -> bool {
        let Self {
            svm,
            creator,
            worker_a,
            worker_b,
            arbitrator,
            outsider,
            fee_payer,
            ..
        } = self;
        let signer = match party {
            Party::Creator => creator,
            Party::WorkerA => worker_a,
            Party::WorkerB => worker_b,
            Party::Arbitrator => arbitrator,
            Party::Outsider => outsider,
        };
        send_instruction(svm, fee_payer, instruction, &[signer]).is_ok()
    }

    fn execute(&mut self, action: Action) -> bool {
        match action {
            Action::Assign(party) => {
                let actor = self.key(party);
                self.send_as(
                    party,
                    assign_worker_instruction(
                        actor,
                        self.task,
                        self.assignment,
                        self.worker_a.pubkey(),
                    ),
                )
            }
            Action::AcceptAssignment(party) => {
                let worker = self.key(party);
                self.send_as(
                    party,
                    accept_assignment_instruction(worker, self.task, self.assignment),
                )
            }
            Action::AcceptTask(party) => {
                let worker = self.key(party);
                self.send_as(party, accept_task_instruction(worker, self.task))
            }
            Action::InitializeResolution(party) => {
                let actor = self.key(party);
                self.send_as(
                    party,
                    initialize_task_resolution_instruction(
                        actor,
                        self.task,
                        self.resolution,
                        self.arbitrator.pubkey(),
                        0,
                    ),
                )
            }
            Action::Fund(party) => {
                let actor = self.key(party);
                self.send_as(party, fund_task_instruction(actor, self.task, self.vault))
            }
            Action::Submit(party) => {
                let worker = self.key(party);
                self.send_as(
                    party,
                    submit_task_instruction(
                        worker,
                        self.task,
                        "ipfs://property-submission".to_string(),
                    ),
                )
            }
            Action::Cancel(party) => {
                let actor = self.key(party);
                self.send_as(party, cancel_task_instruction(actor, self.task))
            }
            Action::Pay(party) => {
                let actor = self.key(party);
                let worker = self.stored_worker_or_default();
                self.send_as(
                    party,
                    pay_task_instruction(actor, self.task, self.vault, worker),
                )
            }
            Action::RefundAfterTimeout(party) => {
                let actor = self.key(party);
                self.send_as(
                    party,
                    refund_task_after_timeout_instruction(actor, self.task, self.vault),
                )
            }
            Action::Reject(party) => {
                let actor = self.key(party);
                self.send_as(
                    party,
                    reject_submission_instruction(
                        actor,
                        self.task,
                        self.resolution,
                        "ipfs://property-rejection".to_string(),
                    ),
                )
            }
            Action::Resolve(party, outcome) => {
                let authority = self.key(party);
                let worker = self.stored_worker_or_default();
                self.send_as(
                    party,
                    resolve_dispute_instruction(
                        authority,
                        self.task,
                        self.creator.pubkey(),
                        worker,
                        self.resolution,
                        self.vault,
                        outcome,
                    ),
                )
            }
            Action::ResolveByAgreement(outcome) => {
                let worker = self.stored_worker_or_default();
                let instruction = resolve_dispute_by_agreement_instruction(
                    self.creator.pubkey(),
                    worker,
                    self.task,
                    self.resolution,
                    self.vault,
                    outcome,
                );
                let Self {
                    svm,
                    creator,
                    worker_a,
                    worker_b,
                    arbitrator,
                    outsider,
                    fee_payer,
                    ..
                } = self;
                let worker_signer = if worker == worker_a.pubkey() {
                    worker_a
                } else if worker == worker_b.pubkey() {
                    worker_b
                } else if worker == arbitrator.pubkey() {
                    arbitrator
                } else {
                    outsider
                };
                send_instruction(svm, fee_payer, instruction, &[creator, worker_signer]).is_ok()
            }
            Action::SettleTaskAfterTimeout => {
                let worker = self.stored_worker_or_default();
                let instruction = if self.svm.get_account(&self.resolution).is_some() {
                    settle_task_after_timeout_with_resolution_instruction(
                        self.fee_payer.pubkey(),
                        self.task,
                        self.creator.pubkey(),
                        worker,
                        self.vault,
                        self.resolution,
                    )
                } else {
                    settle_task_after_timeout_instruction(
                        self.fee_payer.pubkey(),
                        self.task,
                        self.creator.pubkey(),
                        worker,
                        self.vault,
                    )
                };
                send_instruction(&mut self.svm, &self.fee_payer, instruction, &[]).is_ok()
            }
            Action::SettleDisputeAfterTimeout => {
                let worker = self.stored_worker_or_default();
                let instruction = settle_dispute_after_timeout_instruction(
                    self.fee_payer.pubkey(),
                    self.task,
                    self.creator.pubkey(),
                    worker,
                    self.resolution,
                    self.vault,
                );
                send_instruction(&mut self.svm, &self.fee_payer, instruction, &[]).is_ok()
            }
            Action::AdvanceOneSecond => {
                self.now = self.now.checked_add(1).unwrap();
                set_clock_timestamp(&mut self.svm, self.now);
                false
            }
            Action::AdvanceToSubmissionDeadline => {
                if let Some(deadline) = fetch_task(&self.svm, &self.task).submission_deadline {
                    self.now = deadline;
                    set_clock_timestamp(&mut self.svm, deadline);
                }
                false
            }
            Action::AdvanceToReviewDeadline => {
                if let Some(deadline) = fetch_task(&self.svm, &self.task).review_deadline {
                    self.now = deadline;
                    set_clock_timestamp(&mut self.svm, deadline);
                }
                false
            }
            Action::AdvanceToArbitrationDeadline => {
                if self.svm.get_account(&self.resolution).is_some() {
                    if let Some(deadline) =
                        fetch_task_resolution(&self.svm, &self.resolution).arbitration_deadline
                    {
                        self.now = deadline;
                        set_clock_timestamp(&mut self.svm, deadline);
                    }
                }
                false
            }
        }
    }

    fn worker_received(&self) -> u64 {
        let current = balance(&self.svm, &self.worker_a.pubkey())
            .checked_add(balance(&self.svm, &self.worker_b.pubkey()))
            .unwrap();
        current.checked_sub(self.initial_worker_balance).unwrap()
    }

    fn assert_state_invariants(&self) -> TestCaseResult {
        let task = fetch_task(&self.svm, &self.task);
        match task.status {
            TaskStatus::Open | TaskStatus::Assigned => prop_assert!(task.worker.is_none()),
            TaskStatus::Accepted => prop_assert!(task.worker.is_some()),
            TaskStatus::Funded => prop_assert!(task.funded_at.is_some()),
            TaskStatus::Submitted => prop_assert!(task.submission_reference.is_some()),
            TaskStatus::Disputed => {
                prop_assert!(self.svm.get_account(&self.resolution).is_some());
                let resolution = fetch_task_resolution(&self.svm, &self.resolution);
                prop_assert!(resolution.state == ResolutionState::Disputed);
            }
            TaskStatus::Paid | TaskStatus::Cancelled | TaskStatus::Refunded => {}
        }
        if self.svm.get_account(&self.vault).is_some() {
            let vault = fetch_vault(&self.svm, &self.vault);
            prop_assert_eq!(vault.escrowed_lamports, task.reward_amount);
            assert_vault_solvent(&self.svm, &self.vault);
        }
        prop_assert!(self.worker_received() <= task.reward_amount);
        Ok(())
    }
}

fn is_payout_action(action: Action) -> bool {
    matches!(
        action,
        Action::Pay(_)
            | Action::SettleTaskAfterTimeout
            | Action::SettleDisputeAfterTimeout
            | Action::Resolve(_, DisputeOutcome::PayWorker)
            | Action::ResolveByAgreement(DisputeOutcome::PayWorker)
    )
}

fn is_refund_action(action: Action) -> bool {
    matches!(
        action,
        Action::RefundAfterTimeout(_)
            | Action::Resolve(_, DisputeOutcome::RefundCreator)
            | Action::ResolveByAgreement(DisputeOutcome::RefundCreator)
    )
}

fn is_dispute_finish_action(action: Action) -> bool {
    matches!(
        action,
        Action::Resolve(_, _) | Action::ResolveByAgreement(_) | Action::SettleDisputeAfterTimeout
    )
}

fn check_onchain_sequence(actions: &[Action]) -> TestCaseResult {
    let mut harness = OnchainHarness::new();
    let mut payout_count = 0_u8;
    let mut refund_count = 0_u8;
    let mut vault_close_count = 0_u8;
    let mut dispute_finish_count = 0_u8;
    harness.assert_state_invariants()?;

    for &action in actions {
        let task_before = fetch_task(&harness.svm, &harness.task);
        let task_snapshot_before = snapshot_account(&harness.svm, &harness.task);
        let vault_existed = harness.svm.get_account(&harness.vault).is_some();
        let was_disputed = task_before.status == TaskStatus::Disputed;
        let success = harness.execute(action);
        let task_after = fetch_task(&harness.svm, &harness.task);
        let vault_exists = harness.svm.get_account(&harness.vault).is_some();

        if vault_existed && !vault_exists {
            vault_close_count += 1;
        }
        if success && is_payout_action(action) {
            payout_count += 1;
        }
        if success && is_refund_action(action) {
            refund_count += 1;
        }
        if success && is_dispute_finish_action(action) {
            dispute_finish_count += 1;
        }
        if is_terminal(task_before.status) {
            prop_assert!(!success);
            prop_assert_eq!(
                snapshot_account(&harness.svm, &harness.task),
                task_snapshot_before
            );
            prop_assert!(task_after.status == task_before.status);
        }
        if was_disputed && matches!(action, Action::Pay(_) | Action::SettleTaskAfterTimeout) {
            prop_assert!(!success);
        }
        if success {
            if let Action::AcceptAssignment(party) = action {
                prop_assert_eq!(party, Party::WorkerA);
            }
        }

        prop_assert!(payout_count <= 1);
        prop_assert!(refund_count <= 1);
        prop_assert!(vault_close_count <= 1);
        prop_assert!(dispute_finish_count <= 1);
        harness.assert_state_invariants()?;

        if success && is_terminal(task_after.status) {
            let terminal_task = snapshot_account(&harness.svm, &harness.task);
            let worker_received = harness.worker_received();
            prop_assert!(!harness.execute(action));
            prop_assert_eq!(snapshot_account(&harness.svm, &harness.task), terminal_task);
            prop_assert_eq!(harness.worker_received(), worker_received);
        }
    }

    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        max_shrink_iters: 10_000,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    #[test]
    fn model_sequences_preserve_protocol_invariants(
        actions in sequence_strategy(48)
    ) {
        check_model_sequence(&actions)?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        max_shrink_iters: 2_048,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    #[test]
    fn litesvm_sequences_preserve_protocol_invariants(
        actions in sequence_strategy(16)
    ) {
        check_onchain_sequence(&actions)?;
    }
}

#[test]
fn canonical_pdas_reject_all_substitution_attempts() {
    let mut assignment_harness = OnchainHarness::new();
    assert!(assignment_harness.execute(Action::Assign(Party::Creator)));
    let wrong_assignment = Pubkey::new_unique();
    assignment_harness
        .svm
        .set_account(
            wrong_assignment,
            assignment_harness
                .svm
                .get_account(&assignment_harness.assignment)
                .unwrap(),
        )
        .unwrap();
    let task_before = snapshot_account(&assignment_harness.svm, &assignment_harness.task);
    let error = send_instruction(
        &mut assignment_harness.svm,
        &assignment_harness.fee_payer,
        accept_assignment_instruction(
            assignment_harness.worker_a.pubkey(),
            assignment_harness.task,
            wrong_assignment,
        ),
        &[&assignment_harness.worker_a],
    )
    .unwrap_err();
    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_unchanged(
        &assignment_harness.svm,
        &assignment_harness.task,
        &task_before,
    );

    let mut settlement_harness = OnchainHarness::new();
    assert!(settlement_harness.execute(Action::InitializeResolution(Party::Creator)));
    assert!(settlement_harness.execute(Action::AcceptTask(Party::WorkerA)));
    assert!(settlement_harness.execute(Action::Fund(Party::Creator)));
    assert!(settlement_harness.execute(Action::Submit(Party::WorkerA)));

    let wrong_resolution = Pubkey::new_unique();
    settlement_harness
        .svm
        .set_account(
            wrong_resolution,
            settlement_harness
                .svm
                .get_account(&settlement_harness.resolution)
                .unwrap(),
        )
        .unwrap();
    let task_before = snapshot_account(&settlement_harness.svm, &settlement_harness.task);
    let vault_before = snapshot_account(&settlement_harness.svm, &settlement_harness.vault);
    let error = send_instruction(
        &mut settlement_harness.svm,
        &settlement_harness.fee_payer,
        reject_submission_instruction(
            settlement_harness.creator.pubkey(),
            settlement_harness.task,
            wrong_resolution,
            "ipfs://wrong-resolution".to_string(),
        ),
        &[&settlement_harness.creator],
    )
    .unwrap_err();
    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_unchanged(
        &settlement_harness.svm,
        &settlement_harness.task,
        &task_before,
    );
    assert_account_unchanged(
        &settlement_harness.svm,
        &settlement_harness.vault,
        &vault_before,
    );

    let wrong_vault = Pubkey::new_unique();
    settlement_harness
        .svm
        .set_account(
            wrong_vault,
            settlement_harness
                .svm
                .get_account(&settlement_harness.vault)
                .unwrap(),
        )
        .unwrap();
    let task_before = snapshot_account(&settlement_harness.svm, &settlement_harness.task);
    let worker_before = settlement_harness.worker_received();
    let error = send_instruction(
        &mut settlement_harness.svm,
        &settlement_harness.fee_payer,
        pay_task_instruction(
            settlement_harness.creator.pubkey(),
            settlement_harness.task,
            wrong_vault,
            settlement_harness.worker_a.pubkey(),
        ),
        &[&settlement_harness.creator],
    )
    .unwrap_err();
    assert_framework_error(&error, anchor_lang::error::ErrorCode::ConstraintSeeds);
    assert_account_unchanged(
        &settlement_harness.svm,
        &settlement_harness.task,
        &task_before,
    );
    assert_eq!(settlement_harness.worker_received(), worker_before);
    assert_vault_solvent(&settlement_harness.svm, &settlement_harness.vault);
}
