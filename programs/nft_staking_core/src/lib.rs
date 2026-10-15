pub mod constants;
pub mod core_utils;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("h9Ku7LMnUXYojrRPxc85kTdz8YUdKK33RFAWfrzaaxy");

#[program]
pub mod nft_staking_core {
    use super::*;

    pub fn create_collection(
        ctx: Context<CreateCollection>,
        name: String,
        uri: String,
    ) -> Result<()> {
        instructions::create_collection::handle_create_collection(ctx, name, uri)
    }

    pub fn mint_nft(ctx: Context<MintNft>, name: String, uri: String) -> Result<()> {
        instructions::mint_nft::handle_mint_nft(ctx, name, uri)
    }

    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        rewards_per_day: u64,
        freeze_period: i64,
        burn_bonus: u64,
    ) -> Result<()> {
        instructions::initialize_config::handle_initialize_config(
            ctx,
            rewards_per_day,
            freeze_period,
            burn_bonus,
        )
    }

    pub fn stake(ctx: Context<Stake>) -> Result<()> {
        instructions::stake::handle_stake(ctx)
    }

    pub fn unstake(ctx: Context<Unstake>) -> Result<()> {
        instructions::unstake::handle_unstake(ctx)
    }

    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        instructions::claim_rewards::handle_claim_rewards(ctx)
    }

    pub fn burn_staked_nft(ctx: Context<BurnStakedNft>) -> Result<()> {
        instructions::burn_staked_nft::handle_burn_staked_nft(ctx)
    }
}
