use anchor_lang::prelude::*;

use crate::error::ErrorCode;

#[account]
#[derive(InitSpace)]
pub struct CreatorProfile {
    pub task_count: u64,
    pub creator: Pubkey,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum TaskStatus {
    Open,
    Accepted,
    Funded,
    Submitted,
    Paid,
    Cancelled,
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
}

impl Task {
    pub fn accept(&mut self, worker: Pubkey) -> Result<()> {
        require!(self.status.can_accept(), ErrorCode::InvalidStateTransition);
        require!(self.worker.is_none(), ErrorCode::InvalidStateTransition);
        require_keys_neq!(worker, self.creator, ErrorCode::Unauthorized);

        self.worker = Some(worker);
        self.status = TaskStatus::Accepted;

        Ok(())
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

        for terminal_status in [TaskStatus::Paid, TaskStatus::Cancelled] {
            assert!(!terminal_status.can_accept());
            assert!(!terminal_status.can_fund());
            assert!(!terminal_status.can_submit());
            assert!(!terminal_status.can_pay());
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
}
