use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::ErrorCode,
    state::{CreatorProfile, Task, TaskStatus},
};

const MAX_TITLE_BYTES: usize = 100;
const MAX_DESCRIPTION_BYTES: usize = 500;

#[derive(Accounts)]
#[instruction(task_number: u64)]
pub struct CreateTask<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        mut,
        seeds = [
            CREATOR_PROFILE_SEED,
            creator.key().as_ref()
        ],
        bump
    )]
    pub creator_profile: Account<'info, CreatorProfile>,

    #[account(
        init,
        payer = creator,
        space = 8 + Task::INIT_SPACE,
        seeds = [
            TASK_SEED,
            creator.key().as_ref(),
            task_number.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub task: Account<'info, Task>,

    pub system_program: Program<'info, System>,
}

pub fn handle_create_task(
    ctx: Context<CreateTask>,
    task_number: u64,
    title: String,
    description: String,
    reward_amount: u64,
) -> Result<()> {
    require!(reward_amount > 0, ErrorCode::InvalidReward);
    require!(
        !title.trim().is_empty() && title.len() <= MAX_TITLE_BYTES,
        ErrorCode::InvalidTitle
    );
    require!(
        !description.trim().is_empty() && description.len() <= MAX_DESCRIPTION_BYTES,
        ErrorCode::InvalidDescription
    );

    let creator_profile = &mut ctx.accounts.creator_profile;
    let task = &mut ctx.accounts.task;

    let expected_task_number = creator_profile
        .task_count
        .checked_add(1)
        .ok_or(ErrorCode::TaskCountOverflow)?;
    require_eq!(
        task_number,
        expected_task_number,
        ErrorCode::InvalidTaskNumber
    );

    task.task_number = task_number;
    task.creator = ctx.accounts.creator.key();
    task.worker = None;
    task.title = title;
    task.description = description;
    task.reward_amount = reward_amount;
    task.status = TaskStatus::Open;
    task.submission_reference = None;
    task.funded_at = None;

    creator_profile.task_count = task_number;

    msg!("Task created: {}", task.task_number);

    Ok(())
}
