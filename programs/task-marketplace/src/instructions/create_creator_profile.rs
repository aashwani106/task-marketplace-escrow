use anchor_lang::prelude::*;

use crate::{constants::*, state::CreatorProfile};

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
    let creator_profile = &mut ctx.accounts.creator_profile;

    creator_profile.creator = ctx.accounts.creator.key();
    creator_profile.task_count = 0;

    Ok(())
}
