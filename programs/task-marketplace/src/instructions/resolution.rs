use anchor_lang::prelude::*;

use crate::{
    error::ErrorCode,
    state::{DisputeOutcome, EscrowVault, Task, TaskResolution},
};

use super::escrow::{pay_worker, validate_escrow};

pub fn validate_task_resolution(
    task: &Account<Task>,
    task_resolution: &Account<TaskResolution>,
) -> Result<()> {
    task.validate_invariants()?;
    task_resolution.validate_invariants()?;
    require_keys_eq!(
        task_resolution.task,
        task.key(),
        ErrorCode::InvalidResolutionTask
    );
    Ok(())
}

pub fn validate_dispute_accounts(
    task: &Account<Task>,
    task_resolution: &Account<TaskResolution>,
    escrow_vault: &Account<EscrowVault>,
    creator: Pubkey,
    worker: Pubkey,
) -> Result<()> {
    validate_task_resolution(task, task_resolution)?;
    validate_escrow(task, escrow_vault)?;
    require_keys_eq!(creator, task.creator, ErrorCode::Unauthorized);
    require!(task.worker == Some(worker), ErrorCode::Unauthorized);
    require_keys_neq!(creator, worker, ErrorCode::Unauthorized);
    require_keys_neq!(
        task_resolution.arbitration_authority,
        creator,
        ErrorCode::InvalidArbitrationAuthority
    );
    require_keys_neq!(
        task_resolution.arbitration_authority,
        worker,
        ErrorCode::InvalidArbitrationAuthority
    );
    Ok(())
}

pub fn settle_dispute(
    task: &mut Account<Task>,
    escrow_vault: &Account<EscrowVault>,
    worker: &UncheckedAccount,
    outcome: DisputeOutcome,
) -> Result<()> {
    task.resolve_dispute(outcome)?;
    if outcome == DisputeOutcome::PayWorker {
        pay_worker(escrow_vault, worker)?;
    }
    Ok(())
}
