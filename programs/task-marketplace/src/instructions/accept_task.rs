use anchor_lang::prelude::*;

use crate::{
    constants::{EVENT_VERSION, TASK_SEED},
    events::TaskAccepted,
    state::Task,
};

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
    let timestamp = Clock::get()?.unix_timestamp;
    let worker = ctx.accounts.worker.key();
    ctx.accounts.task.accept(worker)?;

    emit!(TaskAccepted {
        version: EVENT_VERSION,
        task: ctx.accounts.task.key(),
        creator: ctx.accounts.task.creator,
        worker,
        actor: worker,
        accepted_at: timestamp,
    });

    Ok(())
}
