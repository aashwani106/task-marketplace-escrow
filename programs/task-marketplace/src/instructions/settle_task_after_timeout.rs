use anchor_lang::prelude::*;

use crate::{
    constants::{TASK_RESOLUTION_SEED, TASK_SEED, VAULT_SEED},
    error::ErrorCode,
    state::{EscrowVault, ResolutionState, Task, TaskResolution},
};

use super::escrow::{pay_worker, validate_escrow};
use super::resolution::validate_task_resolution;

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

    #[account(
        seeds = [TASK_RESOLUTION_SEED, task.key().as_ref()],
        bump = task_resolution.bump
    )]
    pub task_resolution: Option<Account<'info, TaskResolution>>,
}

pub fn handle_settle_task_after_timeout(ctx: Context<SettleTaskAfterTimeout>) -> Result<()> {
    validate_escrow(&ctx.accounts.task, &ctx.accounts.escrow_vault)?;
    require_keys_neq!(
        ctx.accounts.creator.key(),
        ctx.accounts.worker.key(),
        ErrorCode::Unauthorized
    );
    if let Some(task_resolution) = &ctx.accounts.task_resolution {
        validate_task_resolution(&ctx.accounts.task, task_resolution)?;
        require!(
            task_resolution.state == ResolutionState::Ready,
            ErrorCode::InvalidResolutionState
        );
    }

    let timestamp = Clock::get()?.unix_timestamp;
    ctx.accounts.task.pay_after_timeout(timestamp)?;
    pay_worker(&ctx.accounts.escrow_vault, &ctx.accounts.worker)
}
