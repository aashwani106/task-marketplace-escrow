use anchor_lang::prelude::*;

use crate::{constants::TASK_SEED, error::ErrorCode, state::Task};

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
    ctx.accounts.task.cancel()
}
