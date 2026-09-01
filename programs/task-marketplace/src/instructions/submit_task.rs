use anchor_lang::prelude::*;

use crate::{
    constants::{EVENT_VERSION, TASK_SEED},
    error::ErrorCode,
    events::TaskSubmitted,
    state::Task,
};

#[derive(Accounts)]
pub struct SubmitTask<'info> {
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
}

pub fn handle_submit_task(ctx: Context<SubmitTask>, submission_reference: String) -> Result<()> {
    let timestamp = Clock::get()?.unix_timestamp;
    let worker = ctx.accounts.worker.key();
    ctx.accounts
        .task
        .submit(worker, submission_reference, timestamp)?;
    let review_deadline = ctx
        .accounts
        .task
        .review_deadline
        .ok_or(ErrorCode::InvalidStateTransition)?;

    emit!(TaskSubmitted {
        version: EVENT_VERSION,
        task: ctx.accounts.task.key(),
        creator: ctx.accounts.task.creator,
        worker,
        actor: worker,
        submitted_at: timestamp,
        review_deadline,
    });

    Ok(())
}
