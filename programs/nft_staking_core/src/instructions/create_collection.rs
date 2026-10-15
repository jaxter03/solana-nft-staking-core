use anchor_lang::prelude::*;
use mpl_core::{
    instructions::CreateCollectionV2CpiBuilder,
    types::{Attribute, Attributes, Plugin, PluginAuthorityPair},
};

use crate::constants::*;

#[derive(Accounts)]
pub struct CreateCollection<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// New Core collection account.
    #[account(mut)]
    pub collection: Signer<'info>,

    /// CHECK: PDA that becomes the collection's update authority, so the
    /// program can sign plugin updates on the collection and its assets.
    #[account(seeds = [UPDATE_AUTHORITY_SEED, collection.key().as_ref()], bump)]
    pub update_authority: UncheckedAccount<'info>,

    /// CHECK: Metaplex Core program.
    #[account(address = mpl_core::ID)]
    pub core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

/// Create a Core collection owned by the program PDA, seeded with a
/// `total_staked = 0` Attribute that stake/unstake/burn keep in sync.
pub fn handle_create_collection(
    ctx: Context<CreateCollection>,
    name: String,
    uri: String,
) -> Result<()> {
    let collection_key = ctx.accounts.collection.key();
    let seeds: &[&[u8]] = &[
        UPDATE_AUTHORITY_SEED,
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    let core_program = ctx.accounts.core_program.to_account_info();
    let collection = ctx.accounts.collection.to_account_info();
    let update_authority = ctx.accounts.update_authority.to_account_info();
    let payer = ctx.accounts.payer.to_account_info();
    let system_program = ctx.accounts.system_program.to_account_info();

    CreateCollectionV2CpiBuilder::new(&core_program)
        .collection(&collection)
        .update_authority(Some(&update_authority))
        .payer(&payer)
        .system_program(&system_program)
        .name(name)
        .uri(uri)
        .plugins(vec![PluginAuthorityPair {
            plugin: Plugin::Attributes(Attributes {
                attribute_list: vec![Attribute {
                    key: ATTR_TOTAL_STAKED.to_string(),
                    value: "0".to_string(),
                }],
            }),
            authority: None,
        }])
        .invoke_signed(&[seeds])?;

    Ok(())
}
