use anchor_lang::prelude::*;

#[constant]
pub const CREATOR_PROFILE_SEED: &[u8] = b"creator_profile";

#[constant]
pub const TASK_SEED: &[u8] = b"task";

#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

#[constant]
pub const ESCROW_VAULT_VERSION: u8 = 1;

pub const MAX_SUBMISSION_REFERENCE_LENGTH: usize = 200;

#[constant]
pub const SUBMISSION_TIMEOUT_SECONDS: i64 = 7 * 24 * 60 * 60;

#[constant]
pub const REVIEW_TIMEOUT_SECONDS: i64 = 3 * 24 * 60 * 60;
