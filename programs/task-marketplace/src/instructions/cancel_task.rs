use anchor_lang::prelude::*;

use crate::{
    constants::{EVENT_VERSION, TASK_SEED},
    error::ErrorCode,
    events::TaskCancelled,
    state::Task,
};

#[derive(Accounts)]
pub struct CancelTask<'info> {
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
}

pub fn handle_cancel_task(ctx: Context<CancelTask>) -> Result<()> {
    let timestamp = Clock::get()?.unix_timestamp;
    ctx.accounts.task.cancel()?;

    emit!(TaskCancelled {
        version: EVENT_VERSION,
        task: ctx.accounts.task.key(),
        creator: ctx.accounts.task.creator,
        worker: ctx.accounts.task.worker,
        actor: ctx.accounts.creator.key(),
        cancelled_at: timestamp,
    });

    Ok(())
}
