use anchor_lang::prelude::*;

use crate::{
    constants::{EVENT_VERSION, TASK_RESOLUTION_SEED, TASK_SEED, VAULT_SEED},
    error::ErrorCode,
    events::DisputeResolvedByAgreement,
    state::{DisputeOutcome, EscrowVault, Task, TaskResolution},
};

use super::resolution::{settle_dispute, validate_dispute_accounts};

#[derive(Accounts)]
pub struct ResolveDisputeByAgreement<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    /// CHECK: Signer and address constraints bind this account to the stored worker.
    #[account(
        mut,
        signer,
        constraint = task.worker == Some(worker.key()) @ ErrorCode::Unauthorized
    )]
    pub worker: UncheckedAccount<'info>,

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

pub fn handle_resolve_dispute_by_agreement(
    ctx: Context<ResolveDisputeByAgreement>,
    outcome: DisputeOutcome,
) -> Result<()> {
    validate_dispute_accounts(
        &ctx.accounts.task,
        &ctx.accounts.task_resolution,
        &ctx.accounts.escrow_vault,
        ctx.accounts.creator.key(),
        ctx.accounts.worker.key(),
    )?;
    let timestamp = Clock::get()?.unix_timestamp;
    let reward_amount = ctx.accounts.escrow_vault.escrowed_lamports;
    ctx.accounts.task_resolution.resolve_by_agreement(outcome)?;
    settle_dispute(
        &mut ctx.accounts.task,
        &ctx.accounts.escrow_vault,
        &ctx.accounts.worker,
        outcome,
    )?;

    emit!(DisputeResolvedByAgreement {
        version: EVENT_VERSION,
        task: ctx.accounts.task.key(),
        creator: ctx.accounts.creator.key(),
        worker: ctx.accounts.worker.key(),
        actor: ctx.accounts.creator.key(),
        resolved_at: timestamp,
        reward_amount,
        outcome,
    });

    Ok(())
}
