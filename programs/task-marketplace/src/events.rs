use anchor_lang::prelude::*;

use crate::state::DisputeOutcome;

#[event]
pub struct CreatorProfileCreated {
    pub version: u8,
    pub creator_profile: Pubkey,
    pub creator: Pubkey,
    pub actor: Pubkey,
    pub created_at: i64,
}

#[event]
pub struct TaskCreated {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub actor: Pubkey,
    pub created_at: i64,
    pub reward_amount: u64,
}

#[event]
pub struct WorkerAssigned {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Pubkey,
    pub actor: Pubkey,
    pub assigned_at: i64,
}

#[event]
pub struct AssignmentAccepted {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Pubkey,
    pub actor: Pubkey,
    pub accepted_at: i64,
}

#[event]
pub struct TaskAccepted {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Pubkey,
    pub actor: Pubkey,
    pub accepted_at: i64,
}

#[event]
pub struct TaskFunded {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Pubkey,
    pub actor: Pubkey,
    pub funded_at: i64,
    pub submission_deadline: i64,
    pub reward_amount: u64,
}

#[event]
pub struct TaskSubmitted {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Pubkey,
    pub actor: Pubkey,
    pub submitted_at: i64,
    pub review_deadline: i64,
}

#[event]
pub struct TaskCancelled {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Option<Pubkey>,
    pub actor: Pubkey,
    pub cancelled_at: i64,
}

#[event]
pub struct TaskPaid {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Pubkey,
    pub actor: Pubkey,
    pub paid_at: i64,
    pub reward_amount: u64,
}

#[event]
pub struct TaskRefundedAfterTimeout {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Pubkey,
    pub actor: Pubkey,
    pub refunded_at: i64,
    pub submission_deadline: i64,
    pub reward_amount: u64,
}

#[event]
pub struct TaskSettledAfterTimeout {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Pubkey,
    pub actor: Pubkey,
    pub settled_at: i64,
    pub review_deadline: i64,
    pub reward_amount: u64,
}

#[event]
pub struct TaskResolutionInitialized {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub actor: Pubkey,
    pub arbitration_authority: Pubkey,
    pub arbitration_fee_lamports: u64,
    pub initialized_at: i64,
}

#[event]
pub struct SubmissionRejected {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Pubkey,
    pub actor: Pubkey,
    pub rejected_at: i64,
    pub arbitration_deadline: i64,
}

#[event]
pub struct DisputeResolved {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Pubkey,
    pub actor: Pubkey,
    pub resolved_at: i64,
    pub reward_amount: u64,
    pub outcome: DisputeOutcome,
}

#[event]
pub struct DisputeResolvedByAgreement {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Pubkey,
    pub actor: Pubkey,
    pub resolved_at: i64,
    pub reward_amount: u64,
    pub outcome: DisputeOutcome,
}

#[event]
pub struct DisputeSettledAfterTimeout {
    pub version: u8,
    pub task: Pubkey,
    pub creator: Pubkey,
    pub worker: Pubkey,
    pub actor: Pubkey,
    pub settled_at: i64,
    pub reward_amount: u64,
    pub outcome: DisputeOutcome,
}
