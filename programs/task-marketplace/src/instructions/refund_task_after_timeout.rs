use anchor_lang::prelude::*;

use crate::{
    constants::{EVENT_VERSION, TASK_SEED, VAULT_SEED},
    error::ErrorCode,
    events::TaskRefundedAfterTimeout,
    state::{EscrowVault, Task},
};

use super::escrow::validate_escrow;

#[derive(Accounts)]
pub struct RefundTaskAfterTimeout<'info> {
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

    #[account(
        mut,
        seeds = [VAULT_SEED, task.key().as_ref()],
        bump = escrow_vault.bump,
        close = creator
    )]
    pub escrow_vault: Account<'info, EscrowVault>,
}

pub fn handle_refund_task_after_timeout(ctx: Context<RefundTaskAfterTimeout>) -> Result<()> {
    validate_escrow(&ctx.accounts.task, &ctx.accounts.escrow_vault)?;

    let timestamp = Clock::get()?.unix_timestamp;
    let worker = ctx
        .accounts
        .task
        .worker
        .ok_or(ErrorCode::InvalidStateTransition)?;
    let submission_deadline = ctx
        .accounts
        .task
        .submission_deadline
        .ok_or(ErrorCode::InvalidStateTransition)?;
    let reward_amount = ctx.accounts.escrow_vault.escrowed_lamports;
    ctx.accounts.task.refund_after_timeout(timestamp)?;

    emit!(TaskRefundedAfterTimeout {
        version: EVENT_VERSION,
        task: ctx.accounts.task.key(),
        creator: ctx.accounts.creator.key(),
        worker,
        actor: ctx.accounts.creator.key(),
        refunded_at: timestamp,
        submission_deadline,
        reward_amount,
    });

    Ok(())
}
