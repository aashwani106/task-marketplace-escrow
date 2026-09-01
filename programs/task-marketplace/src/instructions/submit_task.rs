use anchor_lang::prelude::*;

use crate::{constants::TASK_SEED, state::Task};

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
    ctx.accounts
        .task
        .submit(ctx.accounts.worker.key(), submission_reference, timestamp)
}
