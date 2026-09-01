use anchor_lang::prelude::*;

#[constant]
pub const CREATOR_PROFILE_SEED: &[u8] = b"creator_profile";

#[constant]
pub const TASK_SEED: &[u8] = b"task";

#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

#[constant]
pub const TASK_RESOLUTION_SEED: &[u8] = b"task_resolution";

#[constant]
pub const WORKER_ASSIGNMENT_SEED: &[u8] = b"worker_assignment";

#[constant]
pub const ESCROW_VAULT_VERSION: u8 = 1;

#[constant]
pub const TASK_RESOLUTION_VERSION: u8 = 1;

#[constant]
pub const WORKER_ASSIGNMENT_VERSION: u8 = 1;

#[constant]
pub const EVENT_VERSION: u8 = 1;

pub const MAX_SUBMISSION_REFERENCE_LENGTH: usize = 200;
pub const MAX_REJECTION_REFERENCE_LENGTH: usize = 200;

#[constant]
pub const SUBMISSION_TIMEOUT_SECONDS: i64 = 7 * 24 * 60 * 60;

#[constant]
pub const REVIEW_TIMEOUT_SECONDS: i64 = 3 * 24 * 60 * 60;

#[constant]
pub const ARBITRATION_TIMEOUT_SECONDS: i64 = 7 * 24 * 60 * 60;
