use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};
use mpl_core::accounts::BaseCollectionV1;

use crate::constants::*;
use crate::error::StakingError;
use crate::state::Config;

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// CHECK: Core collection; its update authority must be our PDA.
    #[account(owner = mpl_core::ID)]
    pub collection: UncheckedAccount<'info>,

    /// CHECK: collection update authority PDA.
    #[account(seeds = [UPDATE_AUTHORITY_SEED, collection.key().as_ref()], bump)]
    pub update_authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = admin,
        space = 8 + Config::INIT_SPACE,
        seeds = [CONFIG_SEED, collection.key().as_ref()],
        bump,
    )]
    pub config: Account<'info, Config>,

    #[account(
        init,
        payer = admin,
        seeds = [REWARDS_SEED, config.key().as_ref()],
        bump,
        mint::decimals = REWARDS_DECIMALS,
        mint::authority = config,
        mint::token_program = token_program,
    )]
    pub rewards_mint: InterfaceAccount<'info, Mint>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_config(
    ctx: Context<InitializeConfig>,
    rewards_per_day: u64,
    freeze_period: i64,
    burn_bonus: u64,
) -> Result<()> {
    let collection = BaseCollectionV1::try_from(&ctx.accounts.collection.to_account_info())
        .map_err(|_| error!(StakingError::InvalidCoreAccount))?;
    require_keys_eq!(
        collection.update_authority,
        ctx.accounts.update_authority.key(),
        StakingError::WrongCollection
    );

    ctx.accounts.config.set_inner(Config {
        admin: ctx.accounts.admin.key(),
        collection: ctx.accounts.collection.key(),
        rewards_per_day,
        freeze_period,
        burn_bonus,
        config_bump: ctx.bumps.config,
        rewards_bump: ctx.bumps.rewards_mint,
    });
    Ok(())
}
