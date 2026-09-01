use anchor_lang::prelude::*;

use crate::{
    constants::{ESCROW_VAULT_VERSION, TASK_SEED, VAULT_SEED},
    error::ErrorCode,
    state::{EscrowVault, Task},
};

#[derive(Accounts)]
pub struct FundTask<'info> {
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
        init,
        payer = creator,
        space = 8 + EscrowVault::INIT_SPACE,
        seeds = [
            VAULT_SEED,
            task.key().as_ref()
        ],
        bump
    )]
    pub escrow_vault: Account<'info, EscrowVault>,

    pub system_program: Program<'info, System>,
}

pub fn handle_fund_task(ctx: Context<FundTask>) -> Result<()> {
    let timestamp = Clock::get()?.unix_timestamp;
    let reward_amount = ctx.accounts.task.reward_amount;
    let task_key = ctx.accounts.task.key();

    ctx.accounts.task.fund(timestamp)?;

    let escrow_vault = &mut ctx.accounts.escrow_vault;
    escrow_vault.version = ESCROW_VAULT_VERSION;
    escrow_vault.bump = ctx.bumps.escrow_vault;
    escrow_vault.task = task_key;
    escrow_vault.escrowed_lamports = reward_amount;
    escrow_vault.reserved = [0; 64];

    let cpi_accounts = anchor_lang::system_program::Transfer {
        from: ctx.accounts.creator.to_account_info(),
        to: escrow_vault.to_account_info(),
    };
    let cpi_context = CpiContext::new(ctx.accounts.system_program.key(), cpi_accounts);
    anchor_lang::system_program::transfer(cpi_context, reward_amount)?;

    let rent_exempt_minimum =
        Rent::get()?.minimum_balance(escrow_vault.to_account_info().data_len());
    let required_balance = rent_exempt_minimum
        .checked_add(escrow_vault.escrowed_lamports)
        .ok_or(ErrorCode::EscrowBalanceOverflow)?;
    require_gte!(
        escrow_vault.to_account_info().lamports(),
        required_balance,
        ErrorCode::EscrowBalanceMismatch
    );

    Ok(())
}
