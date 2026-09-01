use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Reward amount must be greater than zero")]
    InvalidReward,
    #[msg("Title must be non-empty and at most 100 bytes")]
    InvalidTitle,
    #[msg("Description must be non-empty and at most 500 bytes")]
    InvalidDescription,
    #[msg("The requested task state transition is invalid")]
    InvalidStateTransition,
    #[msg("Creator task count overflowed")]
    TaskCountOverflow,
    #[msg("Task number does not match the creator's next task number")]
    InvalidTaskNumber,
    #[msg("The signer is not authorized to perform this action")]
    Unauthorized,
}
