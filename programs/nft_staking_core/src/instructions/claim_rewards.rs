use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::*;
use crate::core_utils::*;
use crate::state::Config;

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
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

    /// CHECK: collection update authority PDA.
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

/// Mint rewards accrued since `last_claimed` and reset the clock. The NFT is
/// not touched: it stays staked and frozen.
pub fn handle_claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
    let user = ctx.accounts.user.to_account_info();
    let asset = ctx.accounts.asset.to_account_info();
    let collection = ctx.accounts.collection.to_account_info();
    load_asset(&asset, &user.key(), &collection.key())?;

    let mut attrs = staked_attributes(&asset)?;
    let last_claimed = get_i64_attr(&attrs, ATTR_LAST_CLAIMED)?;
    let now = Clock::get()?.unix_timestamp;
    let amount = accrued_rewards(last_claimed, now, ctx.accounts.config.rewards_per_day)?;

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
    set_attr(&mut attrs, ATTR_LAST_CLAIMED, now.to_string());
    core.write_attributes(attrs, true, &[ua_seeds])?;

    msg!("claimed {} reward units", amount);
    Ok(())
}
