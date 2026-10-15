use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use mpl_core::instructions::BurnV1CpiBuilder;

use crate::constants::*;
use crate::core_utils::*;
use crate::error::StakingError;
use crate::state::Config;

#[derive(Accounts)]
pub struct BurnStakedNft<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [CONFIG_SEED, collection.key().as_ref()],
        bump = config.config_bump,
        has_one = collection,
    )]
    pub config: Box<Account<'info, Config>>,

    #[account(
        mut,
        seeds = [REWARDS_SEED, config.key().as_ref()],
        bump = config.rewards_bump,
        mint::token_program = token_program,
    )]
    pub rewards_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = rewards_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program,
    )]
    pub user_rewards_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: collection update authority PDA; holds the BurnDelegate.
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

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// Burn a staked NFT for its pending rewards plus the one-time `burn_bonus`.
///
/// The asset is thawed (a frozen asset cannot be burned), then burned by the
/// PDA acting as its BurnDelegate. `total_staked` is decremented.
pub fn handle_burn_staked_nft(ctx: Context<BurnStakedNft>) -> Result<()> {
    let user = ctx.accounts.user.to_account_info();
    let asset = ctx.accounts.asset.to_account_info();
    let collection = ctx.accounts.collection.to_account_info();
    load_asset(&asset, &user.key(), &collection.key())?;

    let attrs = staked_attributes(&asset)?;
    let last_claimed = get_i64_attr(&attrs, ATTR_LAST_CLAIMED)?;
    let now = Clock::get()?.unix_timestamp;
    let accrued = accrued_rewards(last_claimed, now, ctx.accounts.config.rewards_per_day)?;
    let amount = accrued
        .checked_add(ctx.accounts.config.burn_bonus)
        .ok_or(error!(StakingError::Overflow))?;

    let config_seeds: &[&[u8]] = &[
        CONFIG_SEED,
        ctx.accounts.config.collection.as_ref(),
        &[ctx.accounts.config.config_bump],
    ];
    mint_rewards(
        &ctx.accounts.token_program.to_account_info(),
        &ctx.accounts.rewards_mint.to_account_info(),
        &ctx.accounts.user_rewards_ata.to_account_info(),
        &ctx.accounts.config.to_account_info(),
        &[config_seeds],
        amount,
    )?;

    let collection_key = collection.key();
    let ua_seeds: &[&[u8]] = &[
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
    core.set_frozen(false, &[ua_seeds])?;
    BurnV1CpiBuilder::new(&core_program)
        .asset(&asset)
        .collection(Some(&collection))
        .payer(&user)
        .authority(Some(&update_authority))
        .system_program(Some(&system_program))
        .invoke_signed(&[ua_seeds])?;

    core.adjust_total_staked(-1, &[ua_seeds])?;
    msg!(
        "burned, paid {} reward units ({} accrued + {} bonus)",
        amount,
        accrued,
        ctx.accounts.config.burn_bonus
    );
    Ok(())
}
