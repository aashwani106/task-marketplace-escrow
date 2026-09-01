use anchor_lang::prelude::*;

use crate::{
    constants::{EVENT_VERSION, TASK_RESOLUTION_SEED, TASK_SEED},
    error::ErrorCode,
    events::SubmissionRejected,
    state::{Task, TaskResolution},
};

use super::resolution::validate_task_resolution;

#[derive(Accounts)]
pub struct RejectSubmission<'info> {
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
        mut,
        seeds = [TASK_RESOLUTION_SEED, task.key().as_ref()],
        bump = task_resolution.bump
    )]
    pub task_resolution: Account<'info, TaskResolution>,
}

pub fn handle_reject_submission(
    ctx: Context<RejectSubmission>,
    rejection_reference: String,
) -> Result<()> {
    validate_task_resolution(&ctx.accounts.task, &ctx.accounts.task_resolution)?;
    let timestamp = Clock::get()?.unix_timestamp;
    ctx.accounts.task.reject_submission(timestamp)?;
    ctx.accounts
        .task_resolution
        .open_dispute(timestamp, rejection_reference)?;
    let worker = ctx
        .accounts
        .task
        .worker
        .ok_or(ErrorCode::InvalidStateTransition)?;
    let arbitration_deadline = ctx
        .accounts
        .task_resolution
        .arbitration_deadline
        .ok_or(ErrorCode::InvalidResolutionState)?;

    emit!(SubmissionRejected {
        version: EVENT_VERSION,
        task: ctx.accounts.task.key(),
        creator: ctx.accounts.creator.key(),
        worker,
        actor: ctx.accounts.creator.key(),
        rejected_at: timestamp,
        arbitration_deadline,
    });

    Ok(())
}
