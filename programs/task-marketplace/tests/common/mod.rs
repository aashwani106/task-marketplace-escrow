#![allow(dead_code)]

use std::path::PathBuf;

use anchor_lang::{
    prelude::{Clock, Pubkey},
    AccountDeserialize, AccountSerialize, InstructionData, ToAccountMetas,
};
use litesvm::{
    types::{FailedTransactionMetadata, TransactionResult},
    LiteSVM,
};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::{Instruction, InstructionError, Transaction, TransactionError};
use task_marketplace::{
    accounts, instruction,
    state::{CreatorProfile, EscrowVault, Task},
    CREATOR_PROFILE_SEED, ESCROW_VAULT_VERSION, TASK_SEED, VAULT_SEED,
};

pub const DEFAULT_BALANCE: u64 = 10_000_000_000;
pub const DEFAULT_REWARD: u64 = 1_000_000;
pub const DEFAULT_TITLE: &str = "Integration test task";
pub const DEFAULT_DESCRIPTION: &str = "Task created by the LiteSVM integration test suite";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub lamports: u64,
    pub data: Vec<u8>,
    pub owner: Pubkey,
    pub executable: bool,
    pub rent_epoch: u64,
}

pub fn bootstrap() -> LiteSVM {
    let mut svm = LiteSVM::new();
    let program_path = program_path();
    svm.add_program_from_file(task_marketplace::ID, &program_path)
        .unwrap_or_else(|error| {
            panic!(
                "failed to load {}: {error}; run `anchor build` before integration tests",
                program_path.display()
            )
        });
    svm
}

fn program_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/task_marketplace.so")
}

pub fn funded_keypair(svm: &mut LiteSVM) -> Keypair {
    funded_keypair_with_balance(svm, DEFAULT_BALANCE)
}

pub fn funded_keypair_with_balance(svm: &mut LiteSVM, lamports: u64) -> Keypair {
    let keypair = Keypair::new();
    svm.airdrop(&keypair.pubkey(), lamports).unwrap();
    keypair
}

pub fn creator_profile_pda(creator: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[CREATOR_PROFILE_SEED, creator.as_ref()],
        &task_marketplace::ID,
    )
}

pub fn task_pda(creator: &Pubkey, task_number: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[TASK_SEED, creator.as_ref(), &task_number.to_le_bytes()],
        &task_marketplace::ID,
    )
}

pub fn vault_pda(task: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, task.as_ref()], &task_marketplace::ID)
}

pub fn create_creator_profile_instruction(creator: Pubkey, creator_profile: Pubkey) -> Instruction {
    Instruction {
        program_id: task_marketplace::ID,
        accounts: accounts::CreateCreatorProfile {
            creator,
            creator_profile,
            system_program: anchor_lang::system_program::ID,
        }
        .to_account_metas(None),
        data: instruction::CreateCreatorProfile {}.data(),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_task_instruction(
    creator: Pubkey,
    creator_profile: Pubkey,
    task: Pubkey,
    task_number: u64,
    title: String,
    description: String,
    reward_amount: u64,
) -> Instruction {
    Instruction {
        program_id: task_marketplace::ID,
        accounts: accounts::CreateTask {
            creator,
            creator_profile,
            task,
            system_program: anchor_lang::system_program::ID,
        }
        .to_account_metas(None),
        data: instruction::CreateTask {
            task_number,
            title,
            description,
            reward_amount,
        }
        .data(),
    }
}

pub fn accept_task_instruction(worker: Pubkey, task: Pubkey) -> Instruction {
    Instruction {
        program_id: task_marketplace::ID,
        accounts: accounts::AcceptTask { worker, task }.to_account_metas(None),
        data: instruction::AcceptTask {}.data(),
    }
}

pub fn cancel_task_instruction(creator: Pubkey, task: Pubkey) -> Instruction {
    Instruction {
        program_id: task_marketplace::ID,
        accounts: accounts::CancelTask { creator, task }.to_account_metas(None),
        data: instruction::CancelTask {}.data(),
    }
}

pub fn fund_task_instruction(creator: Pubkey, task: Pubkey, escrow_vault: Pubkey) -> Instruction {
    Instruction {
        program_id: task_marketplace::ID,
        accounts: accounts::FundTask {
            creator,
            task,
            escrow_vault,
            system_program: anchor_lang::system_program::ID,
        }
        .to_account_metas(None),
        data: instruction::FundTask {}.data(),
    }
}

pub fn pay_task_instruction(
    creator: Pubkey,
    task: Pubkey,
    escrow_vault: Pubkey,
    worker: Pubkey,
) -> Instruction {
    Instruction {
        program_id: task_marketplace::ID,
        accounts: accounts::PayTask {
            creator,
            task,
            escrow_vault,
            worker,
        }
        .to_account_metas(None),
        data: instruction::PayTask {}.data(),
    }
}

pub fn refund_task_after_timeout_instruction(
    creator: Pubkey,
    task: Pubkey,
    escrow_vault: Pubkey,
) -> Instruction {
    Instruction {
        program_id: task_marketplace::ID,
        accounts: accounts::RefundTaskAfterTimeout {
            creator,
            task,
            escrow_vault,
        }
        .to_account_metas(None),
        data: instruction::RefundTaskAfterTimeout {}.data(),
    }
}

pub fn settle_task_after_timeout_instruction(
    task: Pubkey,
    creator: Pubkey,
    worker: Pubkey,
    escrow_vault: Pubkey,
) -> Instruction {
    Instruction {
        program_id: task_marketplace::ID,
        accounts: accounts::SettleTaskAfterTimeout {
            task,
            creator,
            worker,
            escrow_vault,
        }
        .to_account_metas(None),
        data: instruction::SettleTaskAfterTimeout {}.data(),
    }
}

pub fn submit_task_instruction(
    worker: Pubkey,
    task: Pubkey,
    submission_reference: String,
) -> Instruction {
    Instruction {
        program_id: task_marketplace::ID,
        accounts: accounts::SubmitTask { worker, task }.to_account_metas(None),
        data: instruction::SubmitTask {
            submission_reference,
        }
        .data(),
    }
}

#[allow(clippy::result_large_err)]
pub fn send_instruction(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instruction: Instruction,
    additional_signers: &[&Keypair],
) -> TransactionResult {
    send_instructions(svm, payer, &[instruction], additional_signers)
}

#[allow(clippy::result_large_err)]
pub fn send_instructions(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instructions: &[Instruction],
    additional_signers: &[&Keypair],
) -> TransactionResult {
    svm.expire_blockhash();
    let mut signers = vec![payer];
    signers.extend(
        additional_signers
            .iter()
            .copied()
            .filter(|signer| signer.pubkey() != payer.pubkey()),
    );
    let transaction = Transaction::new(
        &signers,
        Message::new(instructions, Some(&payer.pubkey())),
        svm.latest_blockhash(),
    );
    svm.send_transaction(transaction)
}

pub fn create_creator_profile(svm: &mut LiteSVM, creator: &Keypair) -> Pubkey {
    let (creator_profile, _) = creator_profile_pda(&creator.pubkey());
    send_instruction(
        svm,
        creator,
        create_creator_profile_instruction(creator.pubkey(), creator_profile),
        &[],
    )
    .unwrap();
    creator_profile
}

pub fn create_task(svm: &mut LiteSVM, creator: &Keypair, task_number: u64) -> Pubkey {
    create_task_with_reward(svm, creator, task_number, DEFAULT_REWARD)
}

pub fn create_task_with_reward(
    svm: &mut LiteSVM,
    creator: &Keypair,
    task_number: u64,
    reward_amount: u64,
) -> Pubkey {
    let (creator_profile, _) = creator_profile_pda(&creator.pubkey());
    let (task, _) = task_pda(&creator.pubkey(), task_number);
    send_instruction(
        svm,
        creator,
        create_task_instruction(
            creator.pubkey(),
            creator_profile,
            task,
            task_number,
            DEFAULT_TITLE.to_string(),
            DEFAULT_DESCRIPTION.to_string(),
            reward_amount,
        ),
        &[],
    )
    .unwrap();
    task
}

pub fn accept_task(svm: &mut LiteSVM, worker: &Keypair, task: Pubkey) {
    send_instruction(
        svm,
        worker,
        accept_task_instruction(worker.pubkey(), task),
        &[],
    )
    .unwrap();
}

pub fn fund_task(svm: &mut LiteSVM, creator: &Keypair, task: Pubkey) -> Pubkey {
    let (escrow_vault, _) = vault_pda(&task);
    send_instruction(
        svm,
        creator,
        fund_task_instruction(creator.pubkey(), task, escrow_vault),
        &[],
    )
    .unwrap();
    escrow_vault
}

pub fn pay_task(
    svm: &mut LiteSVM,
    creator: &Keypair,
    task: Pubkey,
    escrow_vault: Pubkey,
    worker: Pubkey,
) -> litesvm::types::TransactionMetadata {
    send_instruction(
        svm,
        creator,
        pay_task_instruction(creator.pubkey(), task, escrow_vault, worker),
        &[],
    )
    .unwrap()
}

pub fn submit_task(svm: &mut LiteSVM, worker: &Keypair, task: Pubkey, submission_reference: &str) {
    send_instruction(
        svm,
        worker,
        submit_task_instruction(worker.pubkey(), task, submission_reference.to_string()),
        &[],
    )
    .unwrap();
}

pub fn set_clock_timestamp(svm: &mut LiteSVM, unix_timestamp: i64) {
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp = unix_timestamp;
    svm.set_sysvar(&clock);
}

pub fn fetch_anchor_account<T: AccountDeserialize>(svm: &LiteSVM, address: &Pubkey) -> T {
    let account = svm
        .get_account(address)
        .unwrap_or_else(|| panic!("account {address} does not exist"));
    assert_eq!(account.owner, task_marketplace::ID);
    T::try_deserialize(&mut account.data.as_slice()).unwrap()
}

pub fn fetch_creator_profile(svm: &LiteSVM, address: &Pubkey) -> CreatorProfile {
    fetch_anchor_account(svm, address)
}

pub fn fetch_task(svm: &LiteSVM, address: &Pubkey) -> Task {
    fetch_anchor_account(svm, address)
}

pub fn fetch_vault(svm: &LiteSVM, address: &Pubkey) -> EscrowVault {
    fetch_anchor_account(svm, address)
}

pub fn overwrite_task(svm: &mut LiteSVM, address: Pubkey, task: &Task) {
    let mut account = svm.get_account(&address).unwrap();
    let account_size = account.data.len();
    let mut data = Vec::with_capacity(account_size);
    task.try_serialize(&mut data).unwrap();
    assert!(data.len() <= account_size);
    data.resize(account_size, 0);
    account.data = data;
    svm.set_account(address, account).unwrap();
}

pub fn overwrite_vault(svm: &mut LiteSVM, address: Pubkey, vault: &EscrowVault) {
    let mut account = svm.get_account(&address).unwrap();
    let account_size = account.data.len();
    let mut data = Vec::with_capacity(account_size);
    vault.try_serialize(&mut data).unwrap();
    assert!(data.len() <= account_size);
    data.resize(account_size, 0);
    account.data = data;
    svm.set_account(address, account).unwrap();
}

pub fn snapshot_account(svm: &LiteSVM, address: &Pubkey) -> Option<AccountSnapshot> {
    svm.get_account(address).map(|account| AccountSnapshot {
        lamports: account.lamports,
        data: account.data,
        owner: account.owner,
        executable: account.executable,
        rent_epoch: account.rent_epoch,
    })
}

pub fn assert_account_unchanged(svm: &LiteSVM, address: &Pubkey, before: &Option<AccountSnapshot>) {
    assert_eq!(&snapshot_account(svm, address), before, "account {address}");
}

pub fn assert_account_absent(svm: &LiteSVM, address: &Pubkey) {
    assert!(
        svm.get_account(address).is_none(),
        "account {address} was unexpectedly created"
    );
}

pub fn balance(svm: &LiteSVM, address: &Pubkey) -> u64 {
    svm.get_balance(address).unwrap_or(0)
}

pub fn set_balance(svm: &mut LiteSVM, address: Pubkey, lamports: u64) {
    let mut account = svm.get_account(&address).unwrap();
    account.lamports = lamports;
    svm.set_account(address, account).unwrap();
}

pub fn transfer_lamports(svm: &mut LiteSVM, sender: &Keypair, recipient: Pubkey, lamports: u64) {
    let instruction = anchor_lang::solana_program::system_instruction::transfer(
        &sender.pubkey(),
        &recipient,
        lamports,
    );
    send_instruction(svm, sender, instruction, &[]).unwrap();
}

pub fn anchor_error_number(error: task_marketplace::error::ErrorCode) -> u32 {
    match anchor_lang::error::Error::from(error) {
        anchor_lang::error::Error::AnchorError(error) => error.error_code_number,
        anchor_lang::error::Error::ProgramError(_) => unreachable!(),
    }
}

pub fn framework_error_number(error: anchor_lang::error::ErrorCode) -> u32 {
    error.into()
}

pub fn assert_custom_error(error: &FailedTransactionMetadata, expected: u32) {
    assert_eq!(
        error.err,
        TransactionError::InstructionError(0, InstructionError::Custom(expected)),
        "logs:\n{}",
        error.meta.pretty_logs()
    );
}

pub fn assert_task_marketplace_error(
    error: &FailedTransactionMetadata,
    expected: task_marketplace::error::ErrorCode,
) {
    assert_custom_error(error, anchor_error_number(expected));
}

pub fn assert_framework_error(
    error: &FailedTransactionMetadata,
    expected: anchor_lang::error::ErrorCode,
) {
    assert_custom_error(error, framework_error_number(expected));
}

pub fn assert_vault_solvent(svm: &LiteSVM, vault_address: &Pubkey) {
    let account = svm.get_account(vault_address).unwrap();
    let vault = fetch_vault(svm, vault_address);
    let rent_exempt_minimum = svm.minimum_balance_for_rent_exemption(account.data.len());
    let required = rent_exempt_minimum
        .checked_add(vault.escrowed_lamports)
        .unwrap();
    assert!(account.lamports >= required);
}

pub fn assert_vault_solvent_if_present(svm: &LiteSVM, vault_address: &Pubkey) {
    if svm.get_account(vault_address).is_some() {
        assert_vault_solvent(svm, vault_address);
    }
}

pub fn assert_vault_solvency_preserved_if_initially_solvent(
    svm: &LiteSVM,
    vault_address: &Pubkey,
    before: &Option<AccountSnapshot>,
) {
    let Some(before) = before else {
        return;
    };
    let vault = fetch_vault(svm, vault_address);
    let rent = svm.minimum_balance_for_rent_exemption(before.data.len());
    let required = rent.checked_add(vault.escrowed_lamports).unwrap();
    if before.lamports >= required {
        assert_vault_solvent(svm, vault_address);
    }
}

pub fn assert_vault_metadata(
    svm: &LiteSVM,
    vault_address: &Pubkey,
    task: Pubkey,
    escrowed_lamports: u64,
) {
    let (_, expected_bump) = vault_pda(&task);
    let vault = fetch_vault(svm, vault_address);
    assert_eq!(vault.version, ESCROW_VAULT_VERSION);
    assert_eq!(vault.bump, expected_bump);
    assert_eq!(vault.task, task);
    assert_eq!(vault.escrowed_lamports, escrowed_lamports);
    assert_eq!(vault.reserved, [0; 64]);
}
