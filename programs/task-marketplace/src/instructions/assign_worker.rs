use anchor_lang::prelude::*;

use crate::{
    constants::{EVENT_VERSION, TASK_SEED, WORKER_ASSIGNMENT_SEED, WORKER_ASSIGNMENT_VERSION},
    error::ErrorCode,
    events::WorkerAssigned,
    state::{Task, WorkerAssignment},
};

#[derive(Accounts)]
pub struct AssignWorker<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        mut,
        has_one = creator @ ErrorCode::Unauthorized,
        seeds = [
            TASK_SEED,
            task.creator.as_ref(),
            task.task_number.to_le_bytes().as_ref(),
        ],
        bump
    )]
    pub task: Account<'info, Task>,

    #[account(
        init,
        payer = creator,
        space = 8 + WorkerAssignment::INIT_SPACE,
        seeds = [WORKER_ASSIGNMENT_SEED, task.key().as_ref()],
        bump
    )]
    pub worker_assignment: Account<'info, WorkerAssignment>,

    pub system_program: Program<'info, System>,
}

pub fn handle_assign_worker(ctx: Context<AssignWorker>, selected_worker: Pubkey) -> Result<()> {
    let timestamp = Clock::get()?.unix_timestamp;
    ctx.accounts.task.assign_worker(selected_worker)?;

    let worker_assignment = &mut ctx.accounts.worker_assignment;
    worker_assignment.version = WORKER_ASSIGNMENT_VERSION;
    worker_assignment.bump = ctx.bumps.worker_assignment;
    worker_assignment.task = ctx.accounts.task.key();
    worker_assignment.selected_worker = selected_worker;
    worker_assignment.assigned_at = timestamp;
    worker_assignment.accepted_at = None;
    worker_assignment.reserved = [0; 64];
    worker_assignment.validate_invariants()?;

    emit!(WorkerAssigned {
        version: EVENT_VERSION,
        task: ctx.accounts.task.key(),
        creator: ctx.accounts.creator.key(),
        worker: selected_worker,
        actor: ctx.accounts.creator.key(),
        assigned_at: timestamp,
    });

    Ok(())
}
