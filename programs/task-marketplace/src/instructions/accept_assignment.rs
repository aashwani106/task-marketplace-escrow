use anchor_lang::prelude::*;

use crate::{
    constants::{EVENT_VERSION, TASK_SEED, WORKER_ASSIGNMENT_SEED, WORKER_ASSIGNMENT_VERSION},
    error::ErrorCode,
    events::AssignmentAccepted,
    state::{Task, WorkerAssignment},
};

#[derive(Accounts)]
pub struct AcceptAssignment<'info> {
    pub worker: Signer<'info>,

    #[account(
        mut,
        seeds = [
            TASK_SEED,
            task.creator.as_ref(),
            task.task_number.to_le_bytes().as_ref(),
        ],
        bump
    )]
    pub task: Account<'info, Task>,

    #[account(
        mut,
        seeds = [WORKER_ASSIGNMENT_SEED, task.key().as_ref()],
        bump = worker_assignment.bump
    )]
    pub worker_assignment: Account<'info, WorkerAssignment>,
}

pub fn handle_accept_assignment(ctx: Context<AcceptAssignment>) -> Result<()> {
    ctx.accounts.worker_assignment.validate_invariants()?;
    require_eq!(
        ctx.accounts.worker_assignment.version,
        WORKER_ASSIGNMENT_VERSION,
        ErrorCode::InvalidAssignmentVersion
    );
    require_keys_eq!(
        ctx.accounts.worker_assignment.task,
        ctx.accounts.task.key(),
        ErrorCode::InvalidAssignmentTask
    );

    let worker = ctx.accounts.worker.key();
    let timestamp = Clock::get()?.unix_timestamp;
    ctx.accounts.worker_assignment.accept(worker, timestamp)?;
    ctx.accounts.task.accept_assignment(worker)?;

    emit!(AssignmentAccepted {
        version: EVENT_VERSION,
        task: ctx.accounts.task.key(),
        creator: ctx.accounts.task.creator,
        worker,
        actor: worker,
        accepted_at: timestamp,
    });

    Ok(())
}
