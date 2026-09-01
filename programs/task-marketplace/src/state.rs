use anchor_lang::prelude::*;

use crate::{
    constants::{
        ARBITRATION_TIMEOUT_SECONDS, MAX_REJECTION_REFERENCE_LENGTH,
        MAX_SUBMISSION_REFERENCE_LENGTH, REVIEW_TIMEOUT_SECONDS, SUBMISSION_TIMEOUT_SECONDS,
        TASK_RESOLUTION_VERSION, WORKER_ASSIGNMENT_VERSION,
    },
    error::ErrorCode,
};

#[account]
#[derive(InitSpace)]
pub struct CreatorProfile {
    pub task_count: u64,
    pub creator: Pubkey,
}

#[account]
#[derive(InitSpace)]
pub struct EscrowVault {
    pub version: u8,
    pub bump: u8,
    pub task: Pubkey,
    pub escrowed_lamports: u64,
    pub reserved: [u8; 64],
}

#[account]
#[derive(InitSpace)]
pub struct TaskResolution {
    pub version: u8,
    pub bump: u8,
    pub task: Pubkey,
    pub arbitration_authority: Pubkey,
    pub arbitration_fee_lamports: u64,
    pub state: ResolutionState,
    pub opened_at: Option<i64>,
    pub arbitration_deadline: Option<i64>,
    #[max_len(200)]
    pub rejection_reference: Option<String>,
    pub outcome: Option<DisputeOutcome>,
    pub reserved: [u8; 64],
}

#[account]
#[derive(InitSpace)]
pub struct WorkerAssignment {
    pub version: u8,
    pub bump: u8,
    pub task: Pubkey,
    pub selected_worker: Pubkey,
    pub assigned_at: i64,
    pub accepted_at: Option<i64>,
    pub reserved: [u8; 64],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, InitSpace)]
pub enum ResolutionState {
    Ready,
    Disputed,
    Resolved,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, InitSpace)]
pub enum DisputeOutcome {
    PayWorker,
    RefundCreator,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum TaskStatus {
    Open,
    Accepted,
    Funded,
    Submitted,
    Paid,
    Cancelled,
    Disputed,
    Refunded,
    Assigned,
}

impl TaskStatus {
    pub fn can_accept(&self) -> bool {
        matches!(self, Self::Open)
    }

    pub fn can_fund(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    pub fn can_submit(&self) -> bool {
        matches!(self, Self::Funded)
    }

    pub fn can_pay(&self) -> bool {
        matches!(self, Self::Submitted)
    }
}

#[account]
#[derive(InitSpace)]
pub struct Task {
    pub task_number: u64,

    pub creator: Pubkey,
    pub worker: Option<Pubkey>,

    #[max_len(100)]
    pub title: String,

    #[max_len(500)]
    pub description: String,

    pub reward_amount: u64,

    pub status: TaskStatus,

    #[max_len(200)]
    pub submission_reference: Option<String>,

    pub funded_at: Option<i64>,

    pub submission_deadline: Option<i64>,
    pub review_deadline: Option<i64>,
}

impl Task {
    pub fn validate_invariants(&self) -> Result<()> {
        match self.status {
            TaskStatus::Open => {
                require!(self.worker.is_none(), ErrorCode::InvalidStateTransition);
                require!(self.funded_at.is_none(), ErrorCode::InvalidStateTransition);
                require!(
                    self.submission_deadline.is_none(),
                    ErrorCode::InvalidStateTransition
                );
                require!(
                    self.review_deadline.is_none(),
                    ErrorCode::InvalidStateTransition
                );
                require!(
                    self.submission_reference.is_none(),
                    ErrorCode::InvalidStateTransition
                );
            }
            TaskStatus::Assigned => {
                require!(self.worker.is_none(), ErrorCode::InvalidStateTransition);
            }
            TaskStatus::Accepted => {
                require!(self.worker.is_some(), ErrorCode::InvalidStateTransition);
                require!(self.funded_at.is_none(), ErrorCode::InvalidStateTransition);
                require!(
                    self.submission_reference.is_none(),
                    ErrorCode::InvalidStateTransition
                );
            }
            TaskStatus::Funded => {
                require!(self.worker.is_some(), ErrorCode::InvalidStateTransition);
                require!(self.funded_at.is_some(), ErrorCode::InvalidStateTransition);
                require!(
                    self.submission_deadline.is_some(),
                    ErrorCode::InvalidStateTransition
                );
                require!(
                    self.review_deadline.is_none(),
                    ErrorCode::InvalidStateTransition
                );
                require!(
                    self.submission_reference.is_none(),
                    ErrorCode::InvalidStateTransition
                );
            }
            TaskStatus::Submitted | TaskStatus::Disputed => {
                require!(self.worker.is_some(), ErrorCode::InvalidStateTransition);
                require!(self.funded_at.is_some(), ErrorCode::InvalidStateTransition);
                require!(
                    self.submission_deadline.is_some(),
                    ErrorCode::InvalidStateTransition
                );
                require!(
                    self.review_deadline.is_some(),
                    ErrorCode::InvalidStateTransition
                );
                require!(
                    self.submission_reference.is_some(),
                    ErrorCode::InvalidStateTransition
                );
            }
            TaskStatus::Paid => {
                require!(self.worker.is_some(), ErrorCode::InvalidStateTransition);
                require!(
                    self.submission_reference.is_some(),
                    ErrorCode::InvalidStateTransition
                );
            }
            TaskStatus::Refunded => {
                require!(self.worker.is_some(), ErrorCode::InvalidStateTransition);
            }
            TaskStatus::Cancelled => {
                // Historical cancellation shapes are either pre-funding (both funding fields
                // absent, worker optional) or funded-timeout recovery (both funding fields
                // present, worker required). Neither shape may contain submission/review data.
                require!(
                    self.review_deadline.is_none(),
                    ErrorCode::InvalidStateTransition
                );
                require!(
                    self.submission_reference.is_none(),
                    ErrorCode::InvalidStateTransition
                );
                require!(
                    self.funded_at.is_some() == self.submission_deadline.is_some(),
                    ErrorCode::InvalidStateTransition
                );
                if self.funded_at.is_some() {
                    require!(self.worker.is_some(), ErrorCode::InvalidStateTransition);
                }
            }
        }

        Ok(())
    }

    pub fn accept(&mut self, worker: Pubkey) -> Result<()> {
        self.validate_invariants()?;
        require!(self.status.can_accept(), ErrorCode::InvalidStateTransition);
        require!(self.worker.is_none(), ErrorCode::InvalidStateTransition);
        require_keys_neq!(worker, self.creator, ErrorCode::Unauthorized);

        self.worker = Some(worker);
        self.status = TaskStatus::Accepted;

        self.validate_invariants()
    }

    pub fn assign_worker(&mut self, selected_worker: Pubkey) -> Result<()> {
        self.validate_invariants()?;
        require!(
            self.status == TaskStatus::Open,
            ErrorCode::InvalidStateTransition
        );
        require!(self.worker.is_none(), ErrorCode::InvalidStateTransition);
        require_keys_neq!(
            selected_worker,
            Pubkey::default(),
            ErrorCode::InvalidSelectedWorker
        );
        require_keys_neq!(selected_worker, self.creator, ErrorCode::Unauthorized);

        self.status = TaskStatus::Assigned;
        self.validate_invariants()
    }

    pub fn accept_assignment(&mut self, worker: Pubkey) -> Result<()> {
        self.validate_invariants()?;
        require!(
            self.status == TaskStatus::Assigned,
            ErrorCode::InvalidStateTransition
        );
        require!(self.worker.is_none(), ErrorCode::InvalidStateTransition);
        require_keys_neq!(worker, self.creator, ErrorCode::Unauthorized);

        self.worker = Some(worker);
        self.status = TaskStatus::Accepted;
        self.validate_invariants()
    }

    pub fn fund(&mut self, timestamp: i64) -> Result<()> {
        self.validate_invariants()?;
        require!(self.status.can_fund(), ErrorCode::InvalidStateTransition);
        require!(self.worker.is_some(), ErrorCode::InvalidStateTransition);
        require!(self.funded_at.is_none(), ErrorCode::InvalidStateTransition);
        require!(self.reward_amount > 0, ErrorCode::InvalidReward);
        require!(
            self.submission_deadline.is_none(),
            ErrorCode::InvalidStateTransition
        );
        require!(
            self.review_deadline.is_none(),
            ErrorCode::InvalidStateTransition
        );

        let submission_deadline = timestamp
            .checked_add(SUBMISSION_TIMEOUT_SECONDS)
            .ok_or(ErrorCode::DeadlineOverflow)?;

        self.status = TaskStatus::Funded;
        self.funded_at = Some(timestamp);
        self.submission_deadline = Some(submission_deadline);

        self.validate_invariants()
    }

    pub fn submit(
        &mut self,
        worker: Pubkey,
        submission_reference: String,
        timestamp: i64,
    ) -> Result<()> {
        self.validate_invariants()?;
        require!(self.status.can_submit(), ErrorCode::InvalidStateTransition);
        let stored_worker = self.worker.ok_or(ErrorCode::InvalidStateTransition)?;
        require_keys_eq!(stored_worker, worker, ErrorCode::Unauthorized);
        let submission_deadline = self
            .submission_deadline
            .ok_or(ErrorCode::InvalidStateTransition)?;
        require!(
            timestamp < submission_deadline,
            ErrorCode::SubmissionWindowExpired
        );
        require!(
            self.review_deadline.is_none(),
            ErrorCode::InvalidStateTransition
        );
        require!(
            !submission_reference.trim().is_empty()
                && submission_reference.len() <= MAX_SUBMISSION_REFERENCE_LENGTH,
            ErrorCode::InvalidSubmissionReference
        );

        let review_deadline = timestamp
            .checked_add(REVIEW_TIMEOUT_SECONDS)
            .ok_or(ErrorCode::DeadlineOverflow)?;

        self.submission_reference = Some(submission_reference);
        self.review_deadline = Some(review_deadline);
        self.status = TaskStatus::Submitted;

        self.validate_invariants()
    }

    pub fn pay(&mut self) -> Result<()> {
        self.validate_invariants()?;
        require!(self.status.can_pay(), ErrorCode::InvalidStateTransition);
        require!(self.worker.is_some(), ErrorCode::InvalidStateTransition);
        require!(self.funded_at.is_some(), ErrorCode::InvalidStateTransition);
        require!(
            self.submission_deadline.is_some(),
            ErrorCode::InvalidStateTransition
        );
        require!(
            self.review_deadline.is_some(),
            ErrorCode::InvalidStateTransition
        );
        require!(
            self.submission_reference.is_some(),
            ErrorCode::InvalidStateTransition
        );

        self.status = TaskStatus::Paid;

        self.validate_invariants()
    }

    pub fn refund_after_timeout(&mut self, timestamp: i64) -> Result<()> {
        self.validate_invariants()?;
        require!(
            self.status == TaskStatus::Funded,
            ErrorCode::InvalidStateTransition
        );
        require!(self.worker.is_some(), ErrorCode::InvalidStateTransition);
        require!(self.funded_at.is_some(), ErrorCode::InvalidStateTransition);
        require!(
            self.submission_reference.is_none(),
            ErrorCode::InvalidStateTransition
        );
        require!(
            self.review_deadline.is_none(),
            ErrorCode::InvalidStateTransition
        );
        let submission_deadline = self
            .submission_deadline
            .ok_or(ErrorCode::InvalidStateTransition)?;
        require!(
            timestamp >= submission_deadline,
            ErrorCode::SubmissionDeadlineNotReached
        );

        self.status = TaskStatus::Cancelled;

        self.validate_invariants()
    }

    pub fn pay_after_timeout(&mut self, timestamp: i64) -> Result<()> {
        self.validate_invariants()?;
        require!(self.status.can_pay(), ErrorCode::InvalidStateTransition);
        let review_deadline = self
            .review_deadline
            .ok_or(ErrorCode::InvalidStateTransition)?;
        require!(
            timestamp >= review_deadline,
            ErrorCode::ReviewDeadlineNotReached
        );

        self.pay()
    }

    pub fn reject_submission(&mut self, timestamp: i64) -> Result<()> {
        self.validate_invariants()?;
        require!(
            self.status == TaskStatus::Submitted,
            ErrorCode::InvalidStateTransition
        );
        require!(self.worker.is_some(), ErrorCode::InvalidStateTransition);
        require!(
            self.submission_reference.is_some(),
            ErrorCode::InvalidStateTransition
        );
        let review_deadline = self
            .review_deadline
            .ok_or(ErrorCode::InvalidStateTransition)?;
        require!(timestamp < review_deadline, ErrorCode::ReviewWindowExpired);

        self.status = TaskStatus::Disputed;

        self.validate_invariants()
    }

    pub fn resolve_dispute(&mut self, outcome: DisputeOutcome) -> Result<()> {
        self.validate_invariants()?;
        require!(
            self.status == TaskStatus::Disputed,
            ErrorCode::InvalidStateTransition
        );
        require!(self.worker.is_some(), ErrorCode::InvalidStateTransition);
        require!(self.funded_at.is_some(), ErrorCode::InvalidStateTransition);
        require!(
            self.submission_reference.is_some(),
            ErrorCode::InvalidStateTransition
        );

        self.status = match outcome {
            DisputeOutcome::PayWorker => TaskStatus::Paid,
            DisputeOutcome::RefundCreator => TaskStatus::Refunded,
        };

        self.validate_invariants()
    }

    pub fn cancel(&mut self) -> Result<()> {
        self.validate_invariants()?;
        require!(
            matches!(
                self.status,
                TaskStatus::Open | TaskStatus::Assigned | TaskStatus::Accepted
            ),
            ErrorCode::InvalidStateTransition
        );

        self.status = TaskStatus::Cancelled;

        self.validate_invariants()
    }
}

impl WorkerAssignment {
    pub fn validate_invariants(&self) -> Result<()> {
        require_eq!(
            self.version,
            WORKER_ASSIGNMENT_VERSION,
            ErrorCode::InvalidAssignmentVersion
        );
        require_keys_neq!(
            self.task,
            Pubkey::default(),
            ErrorCode::InvalidAssignmentTask
        );
        require_keys_neq!(
            self.selected_worker,
            Pubkey::default(),
            ErrorCode::InvalidSelectedWorker
        );

        // `accepted_at` is the state marker: None is pending and Some is accepted.
        Ok(())
    }

    pub fn accept(&mut self, worker: Pubkey, timestamp: i64) -> Result<()> {
        self.validate_invariants()?;
        require_keys_eq!(worker, self.selected_worker, ErrorCode::Unauthorized);
        require!(
            self.accepted_at.is_none(),
            ErrorCode::InvalidAssignmentState
        );
        self.accepted_at = Some(timestamp);
        self.validate_invariants()
    }
}

impl TaskResolution {
    pub fn validate_invariants(&self) -> Result<()> {
        require_eq!(
            self.version,
            TASK_RESOLUTION_VERSION,
            ErrorCode::InvalidResolutionVersion
        );
        require_keys_neq!(
            self.task,
            Pubkey::default(),
            ErrorCode::InvalidResolutionTask
        );
        require_keys_neq!(
            self.arbitration_authority,
            Pubkey::default(),
            ErrorCode::InvalidArbitrationAuthority
        );

        match self.state {
            ResolutionState::Ready => {
                require!(self.outcome.is_none(), ErrorCode::InvalidResolutionState);
            }
            ResolutionState::Disputed => {
                require!(self.opened_at.is_some(), ErrorCode::InvalidResolutionState);
                require!(
                    self.arbitration_deadline.is_some(),
                    ErrorCode::InvalidResolutionState
                );
            }
            ResolutionState::Resolved => {
                require!(self.outcome.is_some(), ErrorCode::InvalidResolutionState);
            }
        }

        Ok(())
    }

    pub fn open_dispute(&mut self, timestamp: i64, rejection_reference: String) -> Result<()> {
        self.validate_invariants()?;
        require!(
            self.state == ResolutionState::Ready,
            ErrorCode::InvalidResolutionState
        );
        require!(self.opened_at.is_none(), ErrorCode::InvalidResolutionState);
        require!(
            self.arbitration_deadline.is_none(),
            ErrorCode::InvalidResolutionState
        );
        require!(self.outcome.is_none(), ErrorCode::InvalidResolutionState);
        require!(
            !rejection_reference.trim().is_empty()
                && rejection_reference.len() <= MAX_REJECTION_REFERENCE_LENGTH,
            ErrorCode::InvalidRejectionReference
        );

        let arbitration_deadline = timestamp
            .checked_add(ARBITRATION_TIMEOUT_SECONDS)
            .ok_or(ErrorCode::DeadlineOverflow)?;

        self.state = ResolutionState::Disputed;
        self.opened_at = Some(timestamp);
        self.arbitration_deadline = Some(arbitration_deadline);
        self.rejection_reference = Some(rejection_reference);

        self.validate_invariants()
    }

    pub fn resolve(&mut self, outcome: DisputeOutcome, timestamp: i64) -> Result<()> {
        self.validate_invariants()?;
        self.require_disputed()?;
        let arbitration_deadline = self
            .arbitration_deadline
            .ok_or(ErrorCode::InvalidResolutionState)?;
        require!(
            timestamp < arbitration_deadline,
            ErrorCode::ArbitrationWindowExpired
        );

        self.finish(outcome)
    }

    pub fn resolve_by_agreement(&mut self, outcome: DisputeOutcome) -> Result<()> {
        self.validate_invariants()?;
        self.require_disputed()?;
        self.finish(outcome)
    }

    pub fn settle_after_timeout(&mut self, timestamp: i64) -> Result<()> {
        self.validate_invariants()?;
        self.require_disputed()?;
        let arbitration_deadline = self
            .arbitration_deadline
            .ok_or(ErrorCode::InvalidResolutionState)?;
        require!(
            timestamp >= arbitration_deadline,
            ErrorCode::ArbitrationDeadlineNotReached
        );

        self.finish(DisputeOutcome::PayWorker)
    }

    fn require_disputed(&self) -> Result<()> {
        require!(
            self.state == ResolutionState::Disputed,
            ErrorCode::InvalidResolutionState
        );
        require!(self.opened_at.is_some(), ErrorCode::InvalidResolutionState);
        require!(
            self.rejection_reference.is_some(),
            ErrorCode::InvalidResolutionState
        );
        require!(self.outcome.is_none(), ErrorCode::InvalidResolutionState);
        Ok(())
    }

    fn finish(&mut self, outcome: DisputeOutcome) -> Result<()> {
        self.state = ResolutionState::Resolved;
        self.outcome = Some(outcome);
        self.validate_invariants()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task_with(status: TaskStatus, worker: Option<Pubkey>) -> Task {
        Task {
            task_number: 1,
            creator: Pubkey::new_unique(),
            worker,
            title: "Test task".to_string(),
            description: "Test description".to_string(),
            reward_amount: 1,
            status,
            submission_reference: None,
            funded_at: None,
            submission_deadline: None,
            review_deadline: None,
        }
    }

    fn assert_error(result: Result<()>, expected: ErrorCode) {
        let actual = result.expect_err("operation should fail");
        let expected: anchor_lang::error::Error = expected.into();

        match (actual, expected) {
            (
                anchor_lang::error::Error::AnchorError(actual),
                anchor_lang::error::Error::AnchorError(expected),
            ) => assert_eq!(actual.error_code_number, expected.error_code_number),
            _ => panic!("expected an Anchor error"),
        }
    }

    #[test]
    fn task_status_allows_only_the_next_lifecycle_action() {
        assert!(TaskStatus::Open.can_accept());
        assert!(TaskStatus::Accepted.can_fund());
        assert!(TaskStatus::Funded.can_submit());
        assert!(TaskStatus::Submitted.can_pay());

        for terminal_status in [
            TaskStatus::Paid,
            TaskStatus::Cancelled,
            TaskStatus::Disputed,
            TaskStatus::Refunded,
            TaskStatus::Assigned,
        ] {
            assert!(!terminal_status.can_accept());
            assert!(!terminal_status.can_fund());
            assert!(!terminal_status.can_submit());
            assert!(!terminal_status.can_pay());
        }
    }

    #[test]
    fn task_status_variants_are_append_only() {
        for (status, expected_discriminant) in [
            (TaskStatus::Open, 0),
            (TaskStatus::Accepted, 1),
            (TaskStatus::Funded, 2),
            (TaskStatus::Submitted, 3),
            (TaskStatus::Paid, 4),
            (TaskStatus::Cancelled, 5),
            (TaskStatus::Disputed, 6),
            (TaskStatus::Refunded, 7),
            (TaskStatus::Assigned, 8),
        ] {
            let mut bytes = Vec::new();
            status.serialize(&mut bytes).unwrap();
            assert_eq!(bytes, vec![expected_discriminant]);
        }
    }

    #[test]
    fn open_task_can_be_accepted() {
        let mut task = task_with(TaskStatus::Open, None);
        let worker = Pubkey::new_unique();

        task.accept(worker).unwrap();

        assert_eq!(task.worker, Some(worker));
        assert!(task.status == TaskStatus::Accepted);
    }

    #[test]
    fn accepted_task_cannot_be_accepted_again() {
        let mut task = task_with(TaskStatus::Open, None);
        let worker = Pubkey::new_unique();
        task.accept(worker).unwrap();

        assert_error(task.accept(worker), ErrorCode::InvalidStateTransition);
        assert_eq!(task.worker, Some(worker));
        assert!(task.status == TaskStatus::Accepted);
    }

    #[test]
    fn second_worker_cannot_overwrite_the_recorded_worker() {
        let mut task = task_with(TaskStatus::Open, None);
        let first_worker = Pubkey::new_unique();
        let second_worker = Pubkey::new_unique();
        task.accept(first_worker).unwrap();

        assert_error(
            task.accept(second_worker),
            ErrorCode::InvalidStateTransition,
        );
        assert_eq!(task.worker, Some(first_worker));
        assert!(task.status == TaskStatus::Accepted);
    }

    #[test]
    fn creator_cannot_accept_their_own_task() {
        let mut task = task_with(TaskStatus::Open, None);
        let creator = task.creator;

        assert_error(task.accept(creator), ErrorCode::Unauthorized);
        assert_eq!(task.worker, None);
        assert!(task.status == TaskStatus::Open);
    }

    #[test]
    fn all_non_open_states_reject_acceptance() {
        for status in [
            TaskStatus::Accepted,
            TaskStatus::Funded,
            TaskStatus::Submitted,
            TaskStatus::Paid,
            TaskStatus::Cancelled,
            TaskStatus::Disputed,
            TaskStatus::Refunded,
            TaskStatus::Assigned,
        ] {
            let mut task = task_with(status, None);

            assert_error(
                task.accept(Pubkey::new_unique()),
                ErrorCode::InvalidStateTransition,
            );
            assert_eq!(task.worker, None);
            assert!(task.status == status);
        }
    }

    #[test]
    fn inconsistent_open_task_with_worker_rejects_acceptance() {
        let existing_worker = Pubkey::new_unique();
        let mut task = task_with(TaskStatus::Open, Some(existing_worker));

        assert_error(
            task.accept(Pubkey::new_unique()),
            ErrorCode::InvalidStateTransition,
        );
        assert_eq!(task.worker, Some(existing_worker));
        assert!(task.status == TaskStatus::Open);
    }

    #[test]
    fn creator_assignment_reserves_task_for_selected_worker() {
        let selected_worker = Pubkey::new_unique();
        let mut task = task_with(TaskStatus::Open, None);

        task.assign_worker(selected_worker).unwrap();

        assert!(task.status == TaskStatus::Assigned);
        assert_eq!(task.worker, None);
        assert_error(
            task.accept(Pubkey::new_unique()),
            ErrorCode::InvalidStateTransition,
        );
        assert!(task.status == TaskStatus::Assigned);
    }

    #[test]
    fn selected_worker_can_accept_assignment() {
        let selected_worker = Pubkey::new_unique();
        let mut task = task_with(TaskStatus::Open, None);
        task.assign_worker(selected_worker).unwrap();

        task.accept_assignment(selected_worker).unwrap();

        assert!(task.status == TaskStatus::Accepted);
        assert_eq!(task.worker, Some(selected_worker));
    }

    #[test]
    fn invalid_worker_selection_is_rejected() {
        let mut task = task_with(TaskStatus::Open, None);
        assert_error(
            task.assign_worker(Pubkey::default()),
            ErrorCode::InvalidSelectedWorker,
        );
        assert!(task.status == TaskStatus::Open);

        let creator = task.creator;
        assert_error(task.assign_worker(creator), ErrorCode::Unauthorized);
        assert!(task.status == TaskStatus::Open);
    }

    #[test]
    fn worker_assignment_enforces_selected_worker_and_replay_protection() {
        let selected_worker = Pubkey::new_unique();
        let mut assignment = ready_worker_assignment(selected_worker);

        assert_error(
            assignment.accept(Pubkey::new_unique(), 100),
            ErrorCode::Unauthorized,
        );
        assert_eq!(assignment.accepted_at, None);

        assignment.accept(selected_worker, 100).unwrap();
        assert_eq!(assignment.accepted_at, Some(100));

        assert_error(
            assignment.accept(selected_worker, 200),
            ErrorCode::InvalidAssignmentState,
        );
        assert_eq!(assignment.accepted_at, Some(100));
    }

    #[test]
    fn accepted_task_with_worker_can_be_funded() {
        let worker = Pubkey::new_unique();
        let mut task = task_with(TaskStatus::Accepted, Some(worker));

        task.fund(1_234_567).unwrap();

        assert!(task.status == TaskStatus::Funded);
        assert_eq!(task.funded_at, Some(1_234_567));
        assert_eq!(
            task.submission_deadline,
            Some(1_234_567 + SUBMISSION_TIMEOUT_SECONDS)
        );
        assert_eq!(task.review_deadline, None);
        assert_eq!(task.worker, Some(worker));
    }

    #[test]
    fn non_accepted_states_cannot_be_funded() {
        for status in [
            TaskStatus::Open,
            TaskStatus::Funded,
            TaskStatus::Submitted,
            TaskStatus::Paid,
            TaskStatus::Cancelled,
            TaskStatus::Disputed,
            TaskStatus::Refunded,
            TaskStatus::Assigned,
        ] {
            let mut task = task_with(status, Some(Pubkey::new_unique()));

            assert_error(task.fund(100), ErrorCode::InvalidStateTransition);
            assert!(task.status == status);
            assert_eq!(task.funded_at, None);
        }
    }

    #[test]
    fn accepted_task_without_worker_cannot_be_funded() {
        let mut task = task_with(TaskStatus::Accepted, None);

        assert_error(task.fund(100), ErrorCode::InvalidStateTransition);
        assert!(task.status == TaskStatus::Accepted);
        assert_eq!(task.funded_at, None);
    }

    #[test]
    fn task_with_existing_funding_timestamp_cannot_be_funded() {
        let original_timestamp = 50;
        let mut task = task_with(TaskStatus::Accepted, Some(Pubkey::new_unique()));
        task.funded_at = Some(original_timestamp);

        assert_error(task.fund(100), ErrorCode::InvalidStateTransition);
        assert!(task.status == TaskStatus::Accepted);
        assert_eq!(task.funded_at, Some(original_timestamp));
    }

    #[test]
    fn task_with_zero_reward_cannot_be_funded() {
        let mut task = task_with(TaskStatus::Accepted, Some(Pubkey::new_unique()));
        task.reward_amount = 0;

        assert_error(task.fund(100), ErrorCode::InvalidReward);
        assert!(task.status == TaskStatus::Accepted);
        assert_eq!(task.funded_at, None);
    }

    #[test]
    fn funded_task_rejects_replay_and_preserves_original_timestamp() {
        let original_timestamp = 100;
        let mut task = task_with(TaskStatus::Accepted, Some(Pubkey::new_unique()));
        task.fund(original_timestamp).unwrap();

        assert_error(task.fund(200), ErrorCode::InvalidStateTransition);
        assert!(task.status == TaskStatus::Funded);
        assert_eq!(task.funded_at, Some(original_timestamp));
    }

    #[test]
    fn funded_task_can_be_submitted_by_worker() {
        let worker = Pubkey::new_unique();
        let funded_at = 100;
        let mut task = funded_task(worker, funded_at);

        task.submit(worker, "ipfs://submission".to_string(), funded_at + 1)
            .unwrap();

        assert!(task.status == TaskStatus::Submitted);
        assert_eq!(
            task.submission_reference,
            Some("ipfs://submission".to_string())
        );
        assert_eq!(task.funded_at, Some(funded_at));
        assert_eq!(task.worker, Some(worker));
        assert_eq!(
            task.review_deadline,
            Some(funded_at + 1 + REVIEW_TIMEOUT_SECONDS)
        );
    }

    #[test]
    fn wrong_worker_cannot_submit_task() {
        let worker = Pubkey::new_unique();
        let mut task = funded_task(worker, 100);

        assert_error(
            task.submit(Pubkey::new_unique(), "ipfs://submission".to_string(), 101),
            ErrorCode::Unauthorized,
        );
        assert!(task.status == TaskStatus::Funded);
        assert_eq!(task.worker, Some(worker));
        assert_eq!(task.submission_reference, None);
    }

    #[test]
    fn creator_cannot_submit_task() {
        let worker = Pubkey::new_unique();
        let mut task = funded_task(worker, 100);
        let creator = task.creator;

        assert_error(
            task.submit(creator, "ipfs://submission".to_string(), 101),
            ErrorCode::Unauthorized,
        );
        assert!(task.status == TaskStatus::Funded);
        assert_eq!(task.worker, Some(worker));
        assert_eq!(task.submission_reference, None);
    }

    #[test]
    fn submitted_task_rejects_replay() {
        let worker = Pubkey::new_unique();
        let original_reference = "ipfs://original".to_string();
        let mut task = funded_task(worker, 100);
        task.submit(worker, original_reference.clone(), 101)
            .unwrap();

        assert_error(
            task.submit(worker, "ipfs://replacement".to_string(), 102),
            ErrorCode::InvalidStateTransition,
        );
        assert!(task.status == TaskStatus::Submitted);
        assert_eq!(task.submission_reference, Some(original_reference));
    }

    #[test]
    fn empty_submission_reference_is_rejected() {
        let worker = Pubkey::new_unique();

        for submission_reference in ["", "   "] {
            let mut task = funded_task(worker, 100);

            assert_error(
                task.submit(worker, submission_reference.to_string(), 101),
                ErrorCode::InvalidSubmissionReference,
            );
            assert!(task.status == TaskStatus::Funded);
            assert_eq!(task.submission_reference, None);
        }
    }

    #[test]
    fn oversized_submission_reference_is_rejected() {
        let worker = Pubkey::new_unique();
        let mut task = funded_task(worker, 100);

        assert_error(
            task.submit(worker, "s".repeat(201), 101),
            ErrorCode::InvalidSubmissionReference,
        );
        assert!(task.status == TaskStatus::Funded);
        assert_eq!(task.submission_reference, None);
    }

    #[test]
    fn two_hundred_byte_submission_reference_is_accepted() {
        let worker = Pubkey::new_unique();
        let reference = "s".repeat(200);
        let mut task = funded_task(worker, 100);

        task.submit(worker, reference.clone(), 101).unwrap();

        assert!(task.status == TaskStatus::Submitted);
        assert_eq!(task.submission_reference, Some(reference));
    }

    #[test]
    fn funding_deadline_overflow_is_rejected_without_mutation() {
        let worker = Pubkey::new_unique();
        let mut task = task_with(TaskStatus::Accepted, Some(worker));

        assert_error(task.fund(i64::MAX), ErrorCode::DeadlineOverflow);
        assert!(task.status == TaskStatus::Accepted);
        assert_eq!(task.funded_at, None);
        assert_eq!(task.submission_deadline, None);
    }

    #[test]
    fn submission_at_deadline_is_rejected_without_mutation() {
        let worker = Pubkey::new_unique();
        let mut task = funded_task(worker, 100);
        let deadline = task.submission_deadline.unwrap();

        assert_error(
            task.submit(worker, "ipfs://submission".to_string(), deadline),
            ErrorCode::SubmissionWindowExpired,
        );
        assert!(task.status == TaskStatus::Funded);
        assert_eq!(task.submission_reference, None);
        assert_eq!(task.review_deadline, None);
    }

    #[test]
    fn review_deadline_overflow_is_rejected_without_mutation() {
        let worker = Pubkey::new_unique();
        let mut task = funded_task(worker, 100);
        task.submission_deadline = Some(i64::MAX);

        assert_error(
            task.submit(worker, "ipfs://submission".to_string(), i64::MAX - 1),
            ErrorCode::DeadlineOverflow,
        );
        assert!(task.status == TaskStatus::Funded);
        assert_eq!(task.submission_reference, None);
        assert_eq!(task.review_deadline, None);
    }

    #[test]
    fn creator_refund_requires_submission_deadline() {
        let worker = Pubkey::new_unique();
        let mut task = funded_task(worker, 100);
        let deadline = task.submission_deadline.unwrap();

        assert_error(
            task.refund_after_timeout(deadline - 1),
            ErrorCode::SubmissionDeadlineNotReached,
        );
        assert!(task.status == TaskStatus::Funded);

        task.refund_after_timeout(deadline).unwrap();
        assert!(task.status == TaskStatus::Cancelled);
        assert_eq!(task.worker, Some(worker));
        assert_eq!(task.funded_at, Some(100));
        assert_eq!(task.submission_reference, None);
        assert_eq!(task.submission_deadline, Some(deadline));
        assert_eq!(task.review_deadline, None);
    }

    #[test]
    fn submitted_task_cannot_use_timeout_refund() {
        let worker = Pubkey::new_unique();
        let mut task = payable_task(worker);
        let deadline = task.submission_deadline.unwrap();

        assert_error(
            task.refund_after_timeout(deadline),
            ErrorCode::InvalidStateTransition,
        );
        assert!(task.status == TaskStatus::Submitted);
    }

    #[test]
    fn permissionless_payment_requires_review_deadline() {
        let worker = Pubkey::new_unique();
        let mut task = payable_task(worker);
        let deadline = task.review_deadline.unwrap();

        assert_error(
            task.pay_after_timeout(deadline - 1),
            ErrorCode::ReviewDeadlineNotReached,
        );
        assert!(task.status == TaskStatus::Submitted);

        task.pay_after_timeout(deadline).unwrap();
        assert!(task.status == TaskStatus::Paid);
    }

    #[test]
    fn timeout_payment_replay_is_rejected() {
        let worker = Pubkey::new_unique();
        let mut task = payable_task(worker);
        let deadline = task.review_deadline.unwrap();
        task.pay_after_timeout(deadline).unwrap();

        assert_error(
            task.pay_after_timeout(deadline),
            ErrorCode::InvalidStateTransition,
        );
        assert!(task.status == TaskStatus::Paid);
    }

    #[test]
    fn submitted_task_can_be_paid() {
        let worker = Pubkey::new_unique();
        let mut task = payable_task(worker);

        task.pay().unwrap();

        assert!(task.status == TaskStatus::Paid);
    }

    #[test]
    fn non_submitted_states_cannot_be_paid() {
        let worker = Pubkey::new_unique();

        for status in [
            TaskStatus::Open,
            TaskStatus::Accepted,
            TaskStatus::Funded,
            TaskStatus::Paid,
            TaskStatus::Cancelled,
            TaskStatus::Disputed,
            TaskStatus::Refunded,
            TaskStatus::Assigned,
        ] {
            let mut task = payable_task(worker);
            task.status = status;

            assert_error(task.pay(), ErrorCode::InvalidStateTransition);
            assert!(task.status == status);
        }
    }

    #[test]
    fn submitted_task_without_worker_cannot_be_paid() {
        let mut task = payable_task(Pubkey::new_unique());
        task.worker = None;

        assert_error(task.pay(), ErrorCode::InvalidStateTransition);
        assert!(task.status == TaskStatus::Submitted);
    }

    #[test]
    fn submitted_task_without_funding_timestamp_cannot_be_paid() {
        let mut task = payable_task(Pubkey::new_unique());
        task.funded_at = None;

        assert_error(task.pay(), ErrorCode::InvalidStateTransition);
        assert!(task.status == TaskStatus::Submitted);
    }

    #[test]
    fn submitted_task_without_submission_reference_cannot_be_paid() {
        let mut task = payable_task(Pubkey::new_unique());
        task.submission_reference = None;

        assert_error(task.pay(), ErrorCode::InvalidStateTransition);
        assert!(task.status == TaskStatus::Submitted);
    }

    #[test]
    fn submitted_task_without_submission_deadline_cannot_be_paid() {
        let mut task = payable_task(Pubkey::new_unique());
        task.submission_deadline = None;

        assert_error(task.pay(), ErrorCode::InvalidStateTransition);
        assert!(task.status == TaskStatus::Submitted);
    }

    #[test]
    fn submitted_task_without_review_deadline_cannot_be_paid() {
        let mut task = payable_task(Pubkey::new_unique());
        task.review_deadline = None;

        assert_error(task.pay(), ErrorCode::InvalidStateTransition);
        assert!(task.status == TaskStatus::Submitted);
    }

    #[test]
    fn paid_task_rejects_replay() {
        let mut task = payable_task(Pubkey::new_unique());
        task.pay().unwrap();

        assert_error(task.pay(), ErrorCode::InvalidStateTransition);
        assert!(task.status == TaskStatus::Paid);
    }

    #[test]
    fn payment_changes_only_task_status() {
        let worker = Pubkey::new_unique();
        let mut task = payable_task(worker);
        let task_number = task.task_number;
        let creator = task.creator;
        let title = task.title.clone();
        let description = task.description.clone();
        let reward_amount = task.reward_amount;
        let submission_reference = task.submission_reference.clone();
        let funded_at = task.funded_at;
        let submission_deadline = task.submission_deadline;
        let review_deadline = task.review_deadline;

        task.pay().unwrap();

        assert!(task.status == TaskStatus::Paid);
        assert_eq!(task.task_number, task_number);
        assert_eq!(task.creator, creator);
        assert_eq!(task.worker, Some(worker));
        assert_eq!(task.title, title);
        assert_eq!(task.description, description);
        assert_eq!(task.reward_amount, reward_amount);
        assert_eq!(task.submission_reference, submission_reference);
        assert_eq!(task.funded_at, funded_at);
        assert_eq!(task.submission_deadline, submission_deadline);
        assert_eq!(task.review_deadline, review_deadline);
    }

    #[test]
    fn submitted_task_can_be_rejected_before_review_deadline() {
        let worker = Pubkey::new_unique();
        let mut task = payable_task(worker);
        let deadline = task.review_deadline.unwrap();
        let submission_reference = task.submission_reference.clone();

        task.reject_submission(deadline - 1).unwrap();

        assert!(task.status == TaskStatus::Disputed);
        assert_eq!(task.worker, Some(worker));
        assert_eq!(task.submission_reference, submission_reference);
        assert_eq!(task.review_deadline, Some(deadline));
    }

    #[test]
    fn rejection_at_review_deadline_is_rejected_without_mutation() {
        let worker = Pubkey::new_unique();
        let mut task = payable_task(worker);
        let deadline = task.review_deadline.unwrap();

        assert_error(
            task.reject_submission(deadline),
            ErrorCode::ReviewWindowExpired,
        );
        assert!(task.status == TaskStatus::Submitted);
    }

    #[test]
    fn only_submitted_tasks_can_be_disputed() {
        for status in [
            TaskStatus::Open,
            TaskStatus::Accepted,
            TaskStatus::Funded,
            TaskStatus::Paid,
            TaskStatus::Cancelled,
            TaskStatus::Disputed,
            TaskStatus::Refunded,
            TaskStatus::Assigned,
        ] {
            let mut task = payable_task(Pubkey::new_unique());
            task.status = status;

            assert_error(
                task.reject_submission(task.review_deadline.unwrap() - 1),
                ErrorCode::InvalidStateTransition,
            );
            assert!(task.status == status);
        }
    }

    #[test]
    fn dispute_outcome_sets_only_the_terminal_status() {
        for (outcome, expected_status) in [
            (DisputeOutcome::PayWorker, TaskStatus::Paid),
            (DisputeOutcome::RefundCreator, TaskStatus::Refunded),
        ] {
            let worker = Pubkey::new_unique();
            let mut task = payable_task(worker);
            task.status = TaskStatus::Disputed;
            let reference = task.submission_reference.clone();

            task.resolve_dispute(outcome).unwrap();

            assert!(task.status == expected_status);
            assert_eq!(task.worker, Some(worker));
            assert_eq!(task.submission_reference, reference);
        }
    }

    #[test]
    fn dispute_resolution_replay_is_rejected() {
        let mut task = payable_task(Pubkey::new_unique());
        task.status = TaskStatus::Disputed;
        task.resolve_dispute(DisputeOutcome::PayWorker).unwrap();

        assert_error(
            task.resolve_dispute(DisputeOutcome::RefundCreator),
            ErrorCode::InvalidStateTransition,
        );
        assert!(task.status == TaskStatus::Paid);
    }

    #[test]
    fn task_resolution_opens_and_records_deadline() {
        let mut resolution = ready_resolution();

        resolution
            .open_dispute(100, "ipfs://rejection".to_string())
            .unwrap();

        assert!(resolution.state == ResolutionState::Disputed);
        assert_eq!(resolution.opened_at, Some(100));
        assert_eq!(
            resolution.arbitration_deadline,
            Some(100 + ARBITRATION_TIMEOUT_SECONDS)
        );
        assert_eq!(
            resolution.rejection_reference,
            Some("ipfs://rejection".to_string())
        );
        assert_eq!(resolution.outcome, None);
    }

    #[test]
    fn invalid_rejection_references_are_rejected() {
        for reference in ["".to_string(), "   ".to_string(), "r".repeat(201)] {
            let mut resolution = ready_resolution();

            assert_error(
                resolution.open_dispute(100, reference),
                ErrorCode::InvalidRejectionReference,
            );
            assert!(resolution.state == ResolutionState::Ready);
            assert_eq!(resolution.opened_at, None);
        }
    }

    #[test]
    fn arbitration_deadline_overflow_is_atomic() {
        let mut resolution = ready_resolution();

        assert_error(
            resolution.open_dispute(i64::MAX, "ipfs://rejection".to_string()),
            ErrorCode::DeadlineOverflow,
        );
        assert!(resolution.state == ResolutionState::Ready);
        assert_eq!(resolution.arbitration_deadline, None);
    }

    #[test]
    fn arbitrator_must_resolve_before_deadline() {
        let mut resolution = disputed_resolution(100);
        let deadline = resolution.arbitration_deadline.unwrap();

        resolution
            .resolve(DisputeOutcome::RefundCreator, deadline - 1)
            .unwrap();
        assert!(resolution.state == ResolutionState::Resolved);
        assert_eq!(resolution.outcome, Some(DisputeOutcome::RefundCreator));

        let mut resolution = disputed_resolution(100);
        assert_error(
            resolution.resolve(DisputeOutcome::PayWorker, deadline),
            ErrorCode::ArbitrationWindowExpired,
        );
        assert!(resolution.state == ResolutionState::Disputed);
        assert_eq!(resolution.outcome, None);
    }

    #[test]
    fn agreement_can_resolve_either_outcome() {
        for outcome in [DisputeOutcome::PayWorker, DisputeOutcome::RefundCreator] {
            let mut resolution = disputed_resolution(100);

            resolution.resolve_by_agreement(outcome).unwrap();

            assert!(resolution.state == ResolutionState::Resolved);
            assert_eq!(resolution.outcome, Some(outcome));
        }
    }

    #[test]
    fn dispute_timeout_is_worker_default_at_boundary() {
        let mut resolution = disputed_resolution(100);
        let deadline = resolution.arbitration_deadline.unwrap();

        assert_error(
            resolution.settle_after_timeout(deadline - 1),
            ErrorCode::ArbitrationDeadlineNotReached,
        );
        assert!(resolution.state == ResolutionState::Disputed);

        resolution.settle_after_timeout(deadline).unwrap();
        assert!(resolution.state == ResolutionState::Resolved);
        assert_eq!(resolution.outcome, Some(DisputeOutcome::PayWorker));
    }

    #[test]
    fn open_task_can_be_cancelled() {
        let mut task = task_with(TaskStatus::Open, None);
        assert_cancellation_preserves_task_fields(&mut task);
    }

    #[test]
    fn accepted_task_can_be_cancelled() {
        let mut task = task_with(TaskStatus::Accepted, Some(Pubkey::new_unique()));
        assert_cancellation_preserves_task_fields(&mut task);
    }

    #[test]
    fn assigned_task_can_be_cancelled() {
        let mut task = task_with(TaskStatus::Assigned, None);
        assert_cancellation_preserves_task_fields(&mut task);
    }

    #[test]
    fn funded_task_cannot_be_cancelled() {
        assert_cancellation_rejected(TaskStatus::Funded);
    }

    #[test]
    fn submitted_task_cannot_be_cancelled() {
        assert_cancellation_rejected(TaskStatus::Submitted);
    }

    #[test]
    fn paid_task_cannot_be_cancelled() {
        assert_cancellation_rejected(TaskStatus::Paid);
    }

    #[test]
    fn every_task_status_has_a_valid_invariant_fixture() {
        for status in [
            TaskStatus::Open,
            TaskStatus::Assigned,
            TaskStatus::Accepted,
            TaskStatus::Funded,
            TaskStatus::Submitted,
            TaskStatus::Disputed,
            TaskStatus::Paid,
            TaskStatus::Refunded,
            TaskStatus::Cancelled,
        ] {
            valid_task_for_status(status).validate_invariants().unwrap();
        }

        let mut accepted_cancellation = valid_task_for_status(TaskStatus::Cancelled);
        accepted_cancellation.worker = Some(Pubkey::new_unique());
        accepted_cancellation.validate_invariants().unwrap();

        let mut timeout_cancellation = valid_task_for_status(TaskStatus::Cancelled);
        timeout_cancellation.worker = Some(Pubkey::new_unique());
        timeout_cancellation.funded_at = Some(10);
        timeout_cancellation.submission_deadline = Some(20);
        timeout_cancellation.validate_invariants().unwrap();
    }

    #[test]
    fn every_task_invariant_rejects_an_invalid_fixture() {
        for (status, mutate) in [
            (
                TaskStatus::Open,
                (|task: &mut Task| task.worker = Some(Pubkey::new_unique())) as fn(&mut Task),
            ),
            (TaskStatus::Open, |task| task.funded_at = Some(1)),
            (TaskStatus::Open, |task| task.submission_deadline = Some(1)),
            (TaskStatus::Open, |task| task.review_deadline = Some(1)),
            (TaskStatus::Open, |task| {
                task.submission_reference = Some("submission".to_string())
            }),
            (TaskStatus::Assigned, |task| {
                task.worker = Some(Pubkey::new_unique())
            }),
            (TaskStatus::Accepted, |task| task.worker = None),
            (TaskStatus::Accepted, |task| task.funded_at = Some(1)),
            (TaskStatus::Accepted, |task| {
                task.submission_reference = Some("submission".to_string())
            }),
            (TaskStatus::Funded, |task| task.worker = None),
            (TaskStatus::Funded, |task| task.funded_at = None),
            (TaskStatus::Funded, |task| task.submission_deadline = None),
            (TaskStatus::Funded, |task| task.review_deadline = Some(1)),
            (TaskStatus::Funded, |task| {
                task.submission_reference = Some("submission".to_string())
            }),
            (TaskStatus::Submitted, |task| task.worker = None),
            (TaskStatus::Submitted, |task| task.funded_at = None),
            (TaskStatus::Submitted, |task| {
                task.submission_deadline = None
            }),
            (TaskStatus::Submitted, |task| task.review_deadline = None),
            (TaskStatus::Submitted, |task| {
                task.submission_reference = None
            }),
            (TaskStatus::Disputed, |task| task.worker = None),
            (TaskStatus::Disputed, |task| task.funded_at = None),
            (TaskStatus::Disputed, |task| task.submission_deadline = None),
            (TaskStatus::Disputed, |task| task.review_deadline = None),
            (TaskStatus::Disputed, |task| {
                task.submission_reference = None
            }),
            (TaskStatus::Paid, |task| task.worker = None),
            (TaskStatus::Paid, |task| task.submission_reference = None),
            (TaskStatus::Refunded, |task| task.worker = None),
            (TaskStatus::Cancelled, |task| task.review_deadline = Some(1)),
            (TaskStatus::Cancelled, |task| {
                task.submission_reference = Some("submission".to_string())
            }),
            (TaskStatus::Cancelled, |task| task.funded_at = Some(1)),
            (TaskStatus::Cancelled, |task| {
                task.submission_deadline = Some(1)
            }),
            (TaskStatus::Cancelled, |task| {
                task.worker = None;
                task.funded_at = Some(1);
                task.submission_deadline = Some(2);
            }),
        ] {
            let mut task = valid_task_for_status(status);
            mutate(&mut task);
            assert_error(
                task.validate_invariants(),
                ErrorCode::InvalidStateTransition,
            );
        }
    }

    #[test]
    fn every_resolution_state_has_a_valid_invariant_fixture() {
        for state in [
            ResolutionState::Ready,
            ResolutionState::Disputed,
            ResolutionState::Resolved,
        ] {
            valid_resolution_for_state(state)
                .validate_invariants()
                .unwrap();
        }
    }

    #[test]
    fn every_resolution_invariant_rejects_an_invalid_fixture() {
        let mut resolution = valid_resolution_for_state(ResolutionState::Ready);
        resolution.version = TASK_RESOLUTION_VERSION.checked_add(1).unwrap();
        assert_error(
            resolution.validate_invariants(),
            ErrorCode::InvalidResolutionVersion,
        );

        let mut resolution = valid_resolution_for_state(ResolutionState::Ready);
        resolution.task = Pubkey::default();
        assert_error(
            resolution.validate_invariants(),
            ErrorCode::InvalidResolutionTask,
        );

        let mut resolution = valid_resolution_for_state(ResolutionState::Ready);
        resolution.arbitration_authority = Pubkey::default();
        assert_error(
            resolution.validate_invariants(),
            ErrorCode::InvalidArbitrationAuthority,
        );

        let mut resolution = valid_resolution_for_state(ResolutionState::Ready);
        resolution.outcome = Some(DisputeOutcome::PayWorker);
        assert_error(
            resolution.validate_invariants(),
            ErrorCode::InvalidResolutionState,
        );

        let mut resolution = valid_resolution_for_state(ResolutionState::Disputed);
        resolution.opened_at = None;
        assert_error(
            resolution.validate_invariants(),
            ErrorCode::InvalidResolutionState,
        );

        let mut resolution = valid_resolution_for_state(ResolutionState::Disputed);
        resolution.arbitration_deadline = None;
        assert_error(
            resolution.validate_invariants(),
            ErrorCode::InvalidResolutionState,
        );

        let mut resolution = valid_resolution_for_state(ResolutionState::Resolved);
        resolution.outcome = None;
        assert_error(
            resolution.validate_invariants(),
            ErrorCode::InvalidResolutionState,
        );
    }

    #[test]
    fn assignment_invariants_cover_pending_accepted_and_invalid_states() {
        let selected_worker = Pubkey::new_unique();
        let mut pending = ready_worker_assignment(selected_worker);
        pending.validate_invariants().unwrap();

        pending.accept(selected_worker, 100).unwrap();
        pending.validate_invariants().unwrap();
        assert_error(
            pending.accept(selected_worker, 101),
            ErrorCode::InvalidAssignmentState,
        );

        let mut invalid = ready_worker_assignment(selected_worker);
        invalid.version = WORKER_ASSIGNMENT_VERSION.checked_add(1).unwrap();
        assert_error(
            invalid.validate_invariants(),
            ErrorCode::InvalidAssignmentVersion,
        );

        let mut invalid = ready_worker_assignment(selected_worker);
        invalid.task = Pubkey::default();
        assert_error(
            invalid.validate_invariants(),
            ErrorCode::InvalidAssignmentTask,
        );

        let mut invalid = ready_worker_assignment(selected_worker);
        invalid.selected_worker = Pubkey::default();
        assert_error(
            invalid.validate_invariants(),
            ErrorCode::InvalidSelectedWorker,
        );
    }

    #[test]
    fn cancelled_task_cannot_be_cancelled_again() {
        assert_cancellation_rejected(TaskStatus::Cancelled);
    }

    fn assert_cancellation_preserves_task_fields(task: &mut Task) {
        let creator = task.creator;
        let worker = task.worker;
        let reward_amount = task.reward_amount;
        let funded_at = task.funded_at;
        let submission_reference = task.submission_reference.clone();
        let submission_deadline = task.submission_deadline;
        let review_deadline = task.review_deadline;

        task.cancel().unwrap();

        assert!(task.status == TaskStatus::Cancelled);
        assert_eq!(task.creator, creator);
        assert_eq!(task.worker, worker);
        assert_eq!(task.reward_amount, reward_amount);
        assert_eq!(task.funded_at, funded_at);
        assert_eq!(task.submission_reference, submission_reference);
        assert_eq!(task.submission_deadline, submission_deadline);
        assert_eq!(task.review_deadline, review_deadline);
    }

    fn assert_cancellation_rejected(status: TaskStatus) {
        let mut task = task_with(status, Some(Pubkey::new_unique()));
        let worker = task.worker;

        assert_error(task.cancel(), ErrorCode::InvalidStateTransition);
        assert!(task.status == status);
        assert_eq!(task.worker, worker);
    }

    fn payable_task(worker: Pubkey) -> Task {
        let mut task = funded_task(worker, 100);
        task.submit(worker, "ipfs://submission".to_string(), 101)
            .unwrap();
        task
    }

    fn valid_task_for_status(status: TaskStatus) -> Task {
        let worker = Pubkey::new_unique();
        let mut task = task_with(TaskStatus::Open, None);
        task.status = status;
        match status {
            TaskStatus::Open | TaskStatus::Assigned | TaskStatus::Cancelled => {}
            TaskStatus::Accepted => task.worker = Some(worker),
            TaskStatus::Funded => {
                task.worker = Some(worker);
                task.funded_at = Some(10);
                task.submission_deadline = Some(20);
            }
            TaskStatus::Submitted | TaskStatus::Disputed | TaskStatus::Paid => {
                task.worker = Some(worker);
                task.funded_at = Some(10);
                task.submission_deadline = Some(20);
                task.review_deadline = Some(30);
                task.submission_reference = Some("submission".to_string());
            }
            TaskStatus::Refunded => task.worker = Some(worker),
        }
        task
    }

    fn valid_resolution_for_state(state: ResolutionState) -> TaskResolution {
        let mut resolution = TaskResolution {
            version: TASK_RESOLUTION_VERSION,
            bump: 255,
            task: Pubkey::new_unique(),
            arbitration_authority: Pubkey::new_unique(),
            arbitration_fee_lamports: 0,
            state,
            opened_at: None,
            arbitration_deadline: None,
            rejection_reference: None,
            outcome: None,
            reserved: [0; 64],
        };
        match state {
            ResolutionState::Ready => {}
            ResolutionState::Disputed => {
                resolution.opened_at = Some(10);
                resolution.arbitration_deadline = Some(20);
                resolution.rejection_reference = Some("rejection".to_string());
            }
            ResolutionState::Resolved => {
                resolution.opened_at = Some(10);
                resolution.arbitration_deadline = Some(20);
                resolution.rejection_reference = Some("rejection".to_string());
                resolution.outcome = Some(DisputeOutcome::PayWorker);
            }
        }
        resolution
    }

    fn funded_task(worker: Pubkey, funded_at: i64) -> Task {
        let mut task = task_with(TaskStatus::Accepted, Some(worker));
        task.fund(funded_at).unwrap();
        task
    }

    fn ready_resolution() -> TaskResolution {
        TaskResolution {
            version: 1,
            bump: 255,
            task: Pubkey::new_unique(),
            arbitration_authority: Pubkey::new_unique(),
            arbitration_fee_lamports: 0,
            state: ResolutionState::Ready,
            opened_at: None,
            arbitration_deadline: None,
            rejection_reference: None,
            outcome: None,
            reserved: [0; 64],
        }
    }

    fn disputed_resolution(timestamp: i64) -> TaskResolution {
        let mut resolution = ready_resolution();
        resolution
            .open_dispute(timestamp, "ipfs://rejection".to_string())
            .unwrap();
        resolution
    }

    fn ready_worker_assignment(selected_worker: Pubkey) -> WorkerAssignment {
        WorkerAssignment {
            version: 1,
            bump: 255,
            task: Pubkey::new_unique(),
            selected_worker,
            assigned_at: 99,
            accepted_at: None,
            reserved: [0; 64],
        }
    }
}
