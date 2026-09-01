use anchor_lang::prelude::*;

use crate::{constants::*, events::CreatorProfileCreated, state::CreatorProfile};

#[derive(Accounts)]
pub struct CreateCreatorProfile<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        init,
        payer = creator,
        space = 8 + CreatorProfile::INIT_SPACE,
        seeds = [
            CREATOR_PROFILE_SEED,
            creator.key().as_ref()
        ],   // means PDA = ["creator_profile", creator_pubkey]
        bump // Find the valid PDA bump automatically
    )]
    pub creator_profile: Account<'info, CreatorProfile>,

    pub system_program: Program<'info, System>,
}

pub fn handle_create_creator_profile(ctx: Context<CreateCreatorProfile>) -> Result<()> {
    let timestamp = Clock::get()?.unix_timestamp;
    let creator = ctx.accounts.creator.key();
    let creator_profile = &mut ctx.accounts.creator_profile;

    creator_profile.creator = creator;
    creator_profile.task_count = 0;

    emit!(CreatorProfileCreated {
        version: EVENT_VERSION,
        creator_profile: creator_profile.key(),
        creator,
        actor: creator,
        created_at: timestamp,
    });

    Ok(())
}
