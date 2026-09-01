use anchor_lang::prelude::*;

use crate::{
    constants::{EVENT_VERSION, TASK_RESOLUTION_SEED, TASK_SEED, VAULT_SEED},
    error::ErrorCode,
    events::DisputeSettledAfterTimeout,
    state::{DisputeOutcome, EscrowVault, Task, TaskResolution},
};

use super::resolution::{settle_dispute, validate_dispute_accounts};

#[derive(Accounts)]
pub struct SettleDisputeAfterTimeout<'info> {
    pub actor: Signer<'info>,

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
    #[account(mut, constraint = task.worker == Some(worker.key()) @ ErrorCode::Unauthorized)]
    pub worker: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [TASK_RESOLUTION_SEED, task.key().as_ref()],
        bump = task_resolution.bump
    )]
    pub task_resolution: Account<'info, TaskResolution>,

    #[account(
        mut,
        seeds = [VAULT_SEED, task.key().as_ref()],
        bump = escrow_vault.bump,
        close = creator
    )]
    pub escrow_vault: Account<'info, EscrowVault>,
}

pub fn handle_settle_dispute_after_timeout(ctx: Context<SettleDisputeAfterTimeout>) -> Result<()> {
    validate_dispute_accounts(
        &ctx.accounts.task,
        &ctx.accounts.task_resolution,
        &ctx.accounts.escrow_vault,
        ctx.accounts.creator.key(),
        ctx.accounts.worker.key(),
    )?;
    let timestamp = Clock::get()?.unix_timestamp;
    let reward_amount = ctx.accounts.escrow_vault.escrowed_lamports;
    ctx.accounts
        .task_resolution
        .settle_after_timeout(timestamp)?;
    settle_dispute(
        &mut ctx.accounts.task,
        &ctx.accounts.escrow_vault,
        &ctx.accounts.worker,
        DisputeOutcome::PayWorker,
    )?;

    emit!(DisputeSettledAfterTimeout {
        version: EVENT_VERSION,
        task: ctx.accounts.task.key(),
        creator: ctx.accounts.creator.key(),
        worker: ctx.accounts.worker.key(),
        actor: ctx.accounts.actor.key(),
        settled_at: timestamp,
        reward_amount,
        outcome: DisputeOutcome::PayWorker,
    });

    Ok(())
}
