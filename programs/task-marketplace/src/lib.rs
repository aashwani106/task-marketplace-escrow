pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use events::*;
pub use instructions::*;
pub use state::*;

declare_id!("FM6bo4u3EMLxMM5NRappPN3ftNzKd7DV5A3z6XFsBQ87");

#[program]
pub mod task_marketplace {
    use super::*;

    pub fn accept_task(ctx: Context<AcceptTask>) -> Result<()> {
        crate::instructions::accept_task::handle_accept_task(ctx)
    }

    pub fn accept_assignment(ctx: Context<AcceptAssignment>) -> Result<()> {
        crate::instructions::accept_assignment::handle_accept_assignment(ctx)
    }

    pub fn assign_worker(ctx: Context<AssignWorker>, selected_worker: Pubkey) -> Result<()> {
        crate::instructions::assign_worker::handle_assign_worker(ctx, selected_worker)
    }

    pub fn cancel_task(ctx: Context<CancelTask>) -> Result<()> {
        crate::instructions::cancel_task::handle_cancel_task(ctx)
    }

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

    pub fn fund_task(ctx: Context<FundTask>) -> Result<()> {
        crate::instructions::fund_task::handle_fund_task(ctx)
    }

    pub fn initialize_task_resolution(
        ctx: Context<InitializeTaskResolution>,
        arbitration_authority: Pubkey,
        arbitration_fee_lamports: u64,
    ) -> Result<()> {
        crate::instructions::initialize_task_resolution::handle_initialize_task_resolution(
            ctx,
            arbitration_authority,
            arbitration_fee_lamports,
        )
    }

    pub fn pay_task(ctx: Context<PayTask>) -> Result<()> {
        crate::instructions::pay_task::handle_pay_task(ctx)
    }

    pub fn reject_submission(
        ctx: Context<RejectSubmission>,
        rejection_reference: String,
    ) -> Result<()> {
        crate::instructions::reject_submission::handle_reject_submission(ctx, rejection_reference)
    }

    pub fn refund_task_after_timeout(ctx: Context<RefundTaskAfterTimeout>) -> Result<()> {
        crate::instructions::refund_task_after_timeout::handle_refund_task_after_timeout(ctx)
    }

    pub fn resolve_dispute(ctx: Context<ResolveDispute>, outcome: DisputeOutcome) -> Result<()> {
        crate::instructions::resolve_dispute::handle_resolve_dispute(ctx, outcome)
    }

    pub fn resolve_dispute_by_agreement(
        ctx: Context<ResolveDisputeByAgreement>,
        outcome: DisputeOutcome,
    ) -> Result<()> {
        crate::instructions::resolve_dispute_by_agreement::handle_resolve_dispute_by_agreement(
            ctx, outcome,
        )
    }

    pub fn settle_dispute_after_timeout(ctx: Context<SettleDisputeAfterTimeout>) -> Result<()> {
        crate::instructions::settle_dispute_after_timeout::handle_settle_dispute_after_timeout(ctx)
    }

    pub fn settle_task_after_timeout(ctx: Context<SettleTaskAfterTimeout>) -> Result<()> {
        crate::instructions::settle_task_after_timeout::handle_settle_task_after_timeout(ctx)
    }

    pub fn submit_task(ctx: Context<SubmitTask>, submission_reference: String) -> Result<()> {
        crate::instructions::submit_task::handle_submit_task(ctx, submission_reference)
    }
}
