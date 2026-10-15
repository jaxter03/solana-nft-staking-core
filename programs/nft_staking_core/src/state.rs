use anchor_lang::prelude::*;

/// Staking parameters for one Core collection.
#[account]
#[derive(InitSpace)]
pub struct Config {
    pub admin: Pubkey,
    pub collection: Pubkey,
    /// Reward base units accrued per staked NFT per day, pro-rated by the second.
    pub rewards_per_day: u64,
    /// Minimum seconds an NFT must stay staked before it can be unstaked.
    pub freeze_period: i64,
    /// One-time reward base units paid on top of accrued rewards when a staked NFT is burned.
    pub burn_bonus: u64,
    pub config_bump: u8,
    pub rewards_bump: u8,
}
