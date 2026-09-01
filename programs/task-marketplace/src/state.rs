use anchor_lang::prelude::*;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
