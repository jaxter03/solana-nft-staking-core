use anchor_lang::prelude::*;
use mpl_core::instructions::CreateV2CpiBuilder;

use crate::constants::*;

#[derive(Accounts)]
pub struct MintNft<'info> {
    /// Pays for and receives the new asset.
    #[account(mut)]
    pub user: Signer<'info>,

    /// New Core asset account.
    #[account(mut)]
    pub asset: Signer<'info>,

    /// CHECK: Core collection; Core verifies it and its update authority.
    #[account(mut, owner = mpl_core::ID)]
    pub collection: UncheckedAccount<'info>,

    /// CHECK: collection update authority PDA, signs to add the asset.
    #[account(seeds = [UPDATE_AUTHORITY_SEED, collection.key().as_ref()], bump)]
    pub update_authority: UncheckedAccount<'info>,

    /// CHECK: Metaplex Core program.
    #[account(address = mpl_core::ID)]
    pub core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handle_mint_nft(ctx: Context<MintNft>, name: String, uri: String) -> Result<()> {
    let collection_key = ctx.accounts.collection.key();
    let seeds: &[&[u8]] = &[
        UPDATE_AUTHORITY_SEED,
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    let core_program = ctx.accounts.core_program.to_account_info();
    let asset = ctx.accounts.asset.to_account_info();
    let collection = ctx.accounts.collection.to_account_info();
    let update_authority = ctx.accounts.update_authority.to_account_info();
    let user = ctx.accounts.user.to_account_info();
    let system_program = ctx.accounts.system_program.to_account_info();

    CreateV2CpiBuilder::new(&core_program)
        .asset(&asset)
        .collection(Some(&collection))
        .authority(Some(&update_authority))
        .payer(&user)
        .owner(Some(&user))
        .system_program(&system_program)
        .name(name)
        .uri(uri)
        .invoke_signed(&[seeds])?;

    Ok(())
}
