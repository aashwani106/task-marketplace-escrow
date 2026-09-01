use anchor_lang::prelude::*;

use crate::{
    constants::{EVENT_VERSION, TASK_SEED, VAULT_SEED},
    error::ErrorCode,
    events::TaskPaid,
    state::{EscrowVault, Task},
};

use super::escrow::{pay_worker, validate_escrow};

#[derive(Accounts)]
pub struct PayTask<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        mut,
        has_one = creator @ ErrorCode::Unauthorized,
        seeds = [
            TASK_SEED,
            task.creator.as_ref(),
            task.task_number.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub task: Account<'info, Task>,

    #[account(
        mut,
        seeds = [
            VAULT_SEED,
            task.key().as_ref()
        ],
        bump = escrow_vault.bump,
        close = creator
    )]
    pub escrow_vault: Account<'info, EscrowVault>,

    /// CHECK: The address is validated against the worker stored in the Task account.
    #[account(mut)]
    pub worker: UncheckedAccount<'info>,
}

pub fn handle_pay_task(ctx: Context<PayTask>) -> Result<()> {
    validate_escrow(&ctx.accounts.task, &ctx.accounts.escrow_vault)?;

    let stored_worker = ctx
        .accounts
        .task
        .worker
        .ok_or(ErrorCode::InvalidStateTransition)?;
    require_keys_eq!(
        ctx.accounts.worker.key(),
        stored_worker,
        ErrorCode::Unauthorized
    );

    let timestamp = Clock::get()?.unix_timestamp;
    let reward_amount = ctx.accounts.escrow_vault.escrowed_lamports;
    ctx.accounts.task.pay()?;
    pay_worker(&ctx.accounts.escrow_vault, &ctx.accounts.worker)?;

    emit!(TaskPaid {
        version: EVENT_VERSION,
        task: ctx.accounts.task.key(),
        creator: ctx.accounts.creator.key(),
        worker: stored_worker,
        actor: ctx.accounts.creator.key(),
        paid_at: timestamp,
        reward_amount,
    });

    Ok(())
}
