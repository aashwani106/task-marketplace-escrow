use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct CreatorProfile {
    pub task_count: u64,
    pub creator: Pubkey,
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    Clone,
    InitSpace
)]
pub enum TaskStatus {
    Open,
    Accepted,
    Funded,
    Submitted,
    Paid,
    Cancelled,
}

 #[account]
 
pub struct Task {
    pub task_number: u64,

    pub creator: Pubkey,
    pub worker: Option<Pubkey>,

    pub title: String,
    pub description: String,

    pub reward_amount: u64,

    pub status: TaskStatus,

    pub submission_reference: Option<String>,
    pub funded_at: Option<i64>,
}