use anchor_lang::prelude::*;
use mpl_core::types::{BurnDelegate, FreezeDelegate, Plugin};

use crate::constants::*;
use crate::core_utils::*;
use crate::error::StakingError;
use crate::state::Config;

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [CONFIG_SEED, collection.key().as_ref()],
        bump = config.config_bump,
        has_one = collection,
    )]
    pub config: Account<'info, Config>,

    /// CHECK: collection update authority PDA; receives the freeze and burn delegations.
    #[account(seeds = [UPDATE_AUTHORITY_SEED, collection.key().as_ref()], bump)]
    pub update_authority: UncheckedAccount<'info>,

    /// CHECK: Core asset; owner and collection are checked in the handler.
    #[account(mut, owner = mpl_core::ID)]
    pub asset: UncheckedAccount<'info>,

    /// CHECK: Core collection, bound to `config` via `has_one`.
    #[account(mut, owner = mpl_core::ID)]
    pub collection: UncheckedAccount<'info>,

    /// CHECK: Metaplex Core program.
    #[account(address = mpl_core::ID)]
    pub core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

/// Freeze the NFT in the user's wallet and start accruing rewards.
///
/// - FreezeDelegate (frozen) and BurnDelegate are added with the PDA as authority.
/// - Asset attributes record `staked`, `staked_at` and `last_claimed`.
/// - The collection's `total_staked` attribute is incremented.
pub fn handle_stake(ctx: Context<Stake>) -> Result<()> {
    let user = ctx.accounts.user.to_account_info();
    let asset = ctx.accounts.asset.to_account_info();
    let collection = ctx.accounts.collection.to_account_info();
    load_asset(&asset, &user.key(), &collection.key())?;

    let existing = asset_attributes(&asset);
    if let Some(attrs) = &existing {
        require!(
            get_attr(attrs, ATTR_STAKED) != Some("true"),
            StakingError::AlreadyStaked
        );
    }

    let collection_key = collection.key();
    let seeds: &[&[u8]] = &[
        UPDATE_AUTHORITY_SEED,
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    let core_program = ctx.accounts.core_program.to_account_info();
    let update_authority = ctx.accounts.update_authority.to_account_info();
    let system_program = ctx.accounts.system_program.to_account_info();
    let core = CoreCtx {
        core_program: &core_program,
        asset: &asset,
        collection: &collection,
        update_authority: &update_authority,
        payer: &user,
        system_program: &system_program,
    };

    let now = Clock::get()?.unix_timestamp;
    let exists = existing.is_some();
    let mut attrs = existing.unwrap_or_default();
    set_attr(&mut attrs, ATTR_STAKED, "true".to_string());
    set_attr(&mut attrs, ATTR_STAKED_AT, now.to_string());
    set_attr(&mut attrs, ATTR_LAST_CLAIMED, now.to_string());
    core.write_attributes(attrs, exists, &[seeds])?;

    core.add_owner_plugin(&user, Plugin::FreezeDelegate(FreezeDelegate { frozen: true }))?;
    core.add_owner_plugin(&user, Plugin::BurnDelegate(BurnDelegate {}))?;

    core.adjust_total_staked(1, &[seeds])?;
    Ok(())
}
