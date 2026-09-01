use anchor_lang::prelude::*;

use crate::{
    constants::ESCROW_VAULT_VERSION,
    error::ErrorCode,
    state::{EscrowVault, Task},
};

pub fn validate_escrow(task: &Account<Task>, escrow_vault: &Account<EscrowVault>) -> Result<()> {
    require_eq!(
        escrow_vault.version,
        ESCROW_VAULT_VERSION,
        ErrorCode::InvalidVaultVersion
    );
    require_keys_eq!(escrow_vault.task, task.key(), ErrorCode::InvalidVaultTask);
    require_eq!(
        escrow_vault.escrowed_lamports,
        task.reward_amount,
        ErrorCode::InvalidEscrowLiability
    );

    let rent_exempt_minimum =
        Rent::get()?.minimum_balance(escrow_vault.to_account_info().data_len());
    let required_balance = rent_exempt_minimum
        .checked_add(escrow_vault.escrowed_lamports)
        .ok_or(ErrorCode::EscrowBalanceOverflow)?;
    require_gte!(
        escrow_vault.get_lamports(),
        required_balance,
        ErrorCode::EscrowBalanceMismatch
    );

    Ok(())
}

pub fn pay_worker(escrow_vault: &Account<EscrowVault>, worker: &UncheckedAccount) -> Result<()> {
    let worker_amount = escrow_vault.escrowed_lamports;
    let vault_balance = escrow_vault.get_lamports();
    let remaining_balance = vault_balance
        .checked_sub(worker_amount)
        .ok_or(ErrorCode::EscrowBalanceMismatch)?;
    worker
        .get_lamports()
        .checked_add(worker_amount)
        .ok_or(ErrorCode::EscrowBalanceOverflow)?;

    escrow_vault.sub_lamports(worker_amount)?;
    worker.add_lamports(worker_amount)?;
    require_eq!(
        escrow_vault.get_lamports(),
        remaining_balance,
        ErrorCode::EscrowBalanceMismatch
    );

    Ok(())
}
