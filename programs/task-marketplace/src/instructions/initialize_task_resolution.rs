use anchor_lang::prelude::*;

use crate::{
    constants::{TASK_RESOLUTION_SEED, TASK_RESOLUTION_VERSION, TASK_SEED},
    error::ErrorCode,
    state::{ResolutionState, Task, TaskResolution, TaskStatus},
};

#[derive(Accounts)]
pub struct InitializeTaskResolution<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
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
        init,
        payer = creator,
        space = 8 + TaskResolution::INIT_SPACE,
        seeds = [TASK_RESOLUTION_SEED, task.key().as_ref()],
        bump
    )]
    pub task_resolution: Account<'info, TaskResolution>,

    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_task_resolution(
    ctx: Context<InitializeTaskResolution>,
    arbitration_authority: Pubkey,
    arbitration_fee_lamports: u64,
) -> Result<()> {
    require!(
        ctx.accounts.task.status == TaskStatus::Open,
        ErrorCode::InvalidStateTransition
    );
    require_keys_neq!(
        arbitration_authority,
        Pubkey::default(),
        ErrorCode::InvalidArbitrationAuthority
    );
    require_keys_neq!(
        arbitration_authority,
        ctx.accounts.creator.key(),
        ErrorCode::InvalidArbitrationAuthority
    );

    let task_resolution = &mut ctx.accounts.task_resolution;
    task_resolution.version = TASK_RESOLUTION_VERSION;
    task_resolution.bump = ctx.bumps.task_resolution;
    task_resolution.task = ctx.accounts.task.key();
    task_resolution.arbitration_authority = arbitration_authority;
    task_resolution.arbitration_fee_lamports = arbitration_fee_lamports;
    task_resolution.state = ResolutionState::Ready;
    task_resolution.opened_at = None;
    task_resolution.arbitration_deadline = None;
    task_resolution.rejection_reference = None;
    task_resolution.outcome = None;
    task_resolution.reserved = [0; 64];

    Ok(())
}
