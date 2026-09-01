use anchor_lang::prelude::*;

use crate::{
    constants::{TASK_SEED, VAULT_SEED},
    error::ErrorCode,
    state::{EscrowVault, Task},
};

use super::escrow::{pay_worker, validate_escrow};

#[derive(Accounts)]
pub struct SettleTaskAfterTimeout<'info> {
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

    /// CHECK: The address is validated against the creator stored in the Task account.
    #[account(mut, address = task.creator @ ErrorCode::Unauthorized)]
    pub creator: UncheckedAccount<'info>,

    /// CHECK: The address is validated against the worker stored in the Task account.
    #[account(
        mut,
        constraint = task.worker == Some(worker.key()) @ ErrorCode::Unauthorized
    )]
    pub worker: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [VAULT_SEED, task.key().as_ref()],
        bump = escrow_vault.bump,
        close = creator
    )]
    pub escrow_vault: Account<'info, EscrowVault>,
}

pub fn handle_settle_task_after_timeout(ctx: Context<SettleTaskAfterTimeout>) -> Result<()> {
    validate_escrow(&ctx.accounts.task, &ctx.accounts.escrow_vault)?;
    require_keys_neq!(
        ctx.accounts.creator.key(),
        ctx.accounts.worker.key(),
        ErrorCode::Unauthorized
    );

    let timestamp = Clock::get()?.unix_timestamp;
    ctx.accounts.task.pay_after_timeout(timestamp)?;
    pay_worker(&ctx.accounts.escrow_vault, &ctx.accounts.worker)
}
