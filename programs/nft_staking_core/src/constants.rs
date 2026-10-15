use anchor_lang::prelude::*;

#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

#[constant]
pub const REWARDS_SEED: &[u8] = b"rewards";

#[constant]
pub const UPDATE_AUTHORITY_SEED: &[u8] = b"update_authority";

/// Reward token decimals.
pub const REWARDS_DECIMALS: u8 = 6;

pub const SECONDS_PER_DAY: i64 = 86_400;

// Attribute keys written onto each staked asset.
pub const ATTR_STAKED: &str = "staked";
pub const ATTR_STAKED_AT: &str = "staked_at";
pub const ATTR_LAST_CLAIMED: &str = "last_claimed";

// Attribute key written onto the collection.
pub const ATTR_TOTAL_STAKED: &str = "total_staked";
