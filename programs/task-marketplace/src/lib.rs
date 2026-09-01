pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("FM6bo4u3EMLxMM5NRappPN3ftNzKd7DV5A3z6XFsBQ87");

#[program]
pub mod task_marketplace {
    use super::*;

    pub fn create_creator_profile(ctx: Context<CreateCreatorProfile>) -> Result<()> {
        crate::instructions::create_creator_profile::handle_create_creator_profile(ctx)
    }

    pub fn create_task(
        ctx: Context<CreateTask>,
        task_number: u64,
        title: String,
        description: String,
        reward_amount: u64,
    ) -> Result<()> {
        crate::instructions::create_task::handle_create_task(
            ctx,
            task_number,
            title,
            description,
            reward_amount,
        )
    }
}
