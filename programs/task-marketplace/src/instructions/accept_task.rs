use anchor_lang::prelude::*;

use crate::{constants::TASK_SEED, state::Task};

#[derive(Accounts)]
pub struct AcceptTask<'info> {
    pub worker: Signer<'info>,

    #[account(
        mut,
        seeds = [
            TASK_SEED,
            task.creator.as_ref(),
            task.task_number.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub task: Account<'info, Task>,
}

pub fn handle_accept_task(ctx: Context<AcceptTask>) -> Result<()> {
    ctx.accounts.task.accept(ctx.accounts.worker.key())
}
