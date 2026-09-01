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
    #[msg("Escrow balance calculation overflowed")]
    EscrowBalanceOverflow,
    #[msg("Escrow vault does not contain the required lamports")]
    EscrowBalanceMismatch,
    #[msg("Invalid submission reference")]
    InvalidSubmissionReference,
    #[msg("Escrow vault version is invalid")]
    InvalidVaultVersion,
    #[msg("Escrow vault does not belong to the task")]
    InvalidVaultTask,
    #[msg("Escrow liability does not match the task reward")]
    InvalidEscrowLiability,
    #[msg("Task deadline calculation overflowed")]
    DeadlineOverflow,
    #[msg("The submission window has expired")]
    SubmissionWindowExpired,
    #[msg("The submission deadline has not been reached")]
    SubmissionDeadlineNotReached,
    #[msg("The review deadline has not been reached")]
    ReviewDeadlineNotReached,
    #[msg("The review window has expired")]
    ReviewWindowExpired,
    #[msg("Task resolution version is invalid")]
    InvalidResolutionVersion,
    #[msg("Task resolution does not belong to the task")]
    InvalidResolutionTask,
    #[msg("The arbitration authority is invalid")]
    InvalidArbitrationAuthority,
    #[msg("The task resolution state is invalid")]
    InvalidResolutionState,
    #[msg("Invalid rejection reference")]
    InvalidRejectionReference,
    #[msg("The arbitration deadline has not been reached")]
    ArbitrationDeadlineNotReached,
    #[msg("The arbitration window has expired")]
    ArbitrationWindowExpired,
    #[msg("Worker assignment version is invalid")]
    InvalidAssignmentVersion,
    #[msg("Worker assignment does not belong to the task")]
    InvalidAssignmentTask,
    #[msg("The selected worker is invalid")]
    InvalidSelectedWorker,
    #[msg("The worker assignment state is invalid")]
    InvalidAssignmentState,
}
