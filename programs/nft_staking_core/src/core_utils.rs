//! Thin helpers around Metaplex Core: reading plugin state off an asset or
//! collection and issuing the plugin CPIs the staking flow needs.

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{mint_to, MintTo};
use mpl_core::{
    accounts::{BaseAssetV1, BaseCollectionV1},
    fetch_plugin,
    instructions::{
        AddPluginV1CpiBuilder, RemovePluginV1CpiBuilder, UpdateCollectionPluginV1CpiBuilder,
        UpdatePluginV1CpiBuilder,
    },
    types::{
        Attribute, Attributes, FreezeDelegate, Key as CoreKey, Plugin, PluginType, UpdateAuthority,
    },
};

use crate::constants::*;
use crate::error::StakingError;

/// Deserialize the asset and check it is owned by `owner` and sits in `collection`.
pub fn load_asset(asset: &AccountInfo, owner: &Pubkey, collection: &Pubkey) -> Result<()> {
    let base =
        BaseAssetV1::try_from(asset).map_err(|_| error!(StakingError::InvalidCoreAccount))?;
    require!(base.key == CoreKey::AssetV1, StakingError::InvalidCoreAccount);
    require_keys_eq!(base.owner, *owner, StakingError::NotOwner);
    require!(
        base.update_authority == UpdateAuthority::Collection(*collection),
        StakingError::WrongCollection
    );
    Ok(())
}

/// Attributes on an asset, or `None` if it has no Attributes plugin yet.
pub fn asset_attributes(asset: &AccountInfo) -> Option<Vec<Attribute>> {
    fetch_plugin::<BaseAssetV1, Attributes>(asset, PluginType::Attributes)
        .ok()
        .map(|(_, attrs, _)| attrs.attribute_list)
}

pub fn get_attr<'a>(attrs: &'a [Attribute], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|a| a.key == key)
        .map(|a| a.value.as_str())
}

pub fn get_i64_attr(attrs: &[Attribute], key: &str) -> Result<i64> {
    get_attr(attrs, key)
        .ok_or(error!(StakingError::InvalidAttribute))?
        .parse()
        .map_err(|_| error!(StakingError::InvalidAttribute))
}

pub fn set_attr(attrs: &mut Vec<Attribute>, key: &str, value: String) {
    match attrs.iter_mut().find(|a| a.key == key) {
        Some(a) => a.value = value,
        None => attrs.push(Attribute {
            key: key.to_string(),
            value,
        }),
    }
}

/// Require the asset to currently be staked and return its attributes.
pub fn staked_attributes(asset: &AccountInfo) -> Result<Vec<Attribute>> {
    let attrs = asset_attributes(asset).ok_or(error!(StakingError::NotStaked))?;
    require!(
        get_attr(&attrs, ATTR_STAKED) == Some("true"),
        StakingError::NotStaked
    );
    Ok(attrs)
}

/// Rewards accrued between `from` and `now`, pro-rated by the second.
pub fn accrued_rewards(from: i64, now: i64, rewards_per_day: u64) -> Result<u64> {
    let elapsed = now.saturating_sub(from).max(0) as u128;
    let amount = elapsed
        .checked_mul(rewards_per_day as u128)
        .ok_or(error!(StakingError::Overflow))?
        / SECONDS_PER_DAY as u128;
    u64::try_from(amount).map_err(|_| error!(StakingError::Overflow))
}

/// Accounts every Core CPI in this program needs. `authority` is the
/// collection's update-authority PDA unless stated otherwise.
pub struct CoreCtx<'a, 'info> {
    pub core_program: &'a AccountInfo<'info>,
    pub asset: &'a AccountInfo<'info>,
    pub collection: &'a AccountInfo<'info>,
    pub update_authority: &'a AccountInfo<'info>,
    pub payer: &'a AccountInfo<'info>,
    pub system_program: &'a AccountInfo<'info>,
}

impl<'a, 'info> CoreCtx<'a, 'info> {
    /// Add or overwrite the asset's Attributes plugin (authority-managed, so
    /// the collection update authority PDA signs).
    pub fn write_attributes(
        &self,
        attrs: Vec<Attribute>,
        exists: bool,
        signer_seeds: &[&[&[u8]]],
    ) -> Result<()> {
        let plugin = Plugin::Attributes(Attributes {
            attribute_list: attrs,
        });
        if exists {
            UpdatePluginV1CpiBuilder::new(self.core_program)
                .asset(self.asset)
                .collection(Some(self.collection))
                .payer(self.payer)
                .authority(Some(self.update_authority))
                .system_program(self.system_program)
                .plugin(plugin)
                .invoke_signed(signer_seeds)?;
        } else {
            AddPluginV1CpiBuilder::new(self.core_program)
                .asset(self.asset)
                .collection(Some(self.collection))
                .payer(self.payer)
                .authority(Some(self.update_authority))
                .system_program(self.system_program)
                .plugin(plugin)
                .invoke_signed(signer_seeds)?;
        }
        Ok(())
    }

    /// Add an owner-managed plugin (the owner signs) and delegate it to the PDA.
    pub fn add_owner_plugin(&self, owner: &'a AccountInfo<'info>, plugin: Plugin) -> Result<()> {
        AddPluginV1CpiBuilder::new(self.core_program)
            .asset(self.asset)
            .collection(Some(self.collection))
            .payer(self.payer)
            .authority(Some(owner))
            .system_program(self.system_program)
            .plugin(plugin)
            .init_authority(mpl_core::types::PluginAuthority::Address {
                address: self.update_authority.key(),
            })
            .invoke()?;
        Ok(())
    }

    /// Remove an owner-managed plugin, signed by the owner.
    pub fn remove_owner_plugin(
        &self,
        owner: &'a AccountInfo<'info>,
        plugin_type: PluginType,
    ) -> Result<()> {
        RemovePluginV1CpiBuilder::new(self.core_program)
            .asset(self.asset)
            .collection(Some(self.collection))
            .payer(self.payer)
            .authority(Some(owner))
            .system_program(self.system_program)
            .plugin_type(plugin_type)
            .invoke()?;
        Ok(())
    }

    /// Freeze or thaw the asset through the FreezeDelegate held by the PDA.
    pub fn set_frozen(&self, frozen: bool, signer_seeds: &[&[&[u8]]]) -> Result<()> {
        UpdatePluginV1CpiBuilder::new(self.core_program)
            .asset(self.asset)
            .collection(Some(self.collection))
            .payer(self.payer)
            .authority(Some(self.update_authority))
            .system_program(self.system_program)
            .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen }))
            .invoke_signed(signer_seeds)?;
        Ok(())
    }

    /// Add `delta` (+1 / -1) to the collection's `total_staked` attribute.
    pub fn adjust_total_staked(&self, delta: i8, signer_seeds: &[&[&[u8]]]) -> Result<()> {
        let (_, stats, _) =
            fetch_plugin::<BaseCollectionV1, Attributes>(self.collection, PluginType::Attributes)
                .map_err(|_| error!(StakingError::MissingCollectionStats))?;
        let mut attrs = stats.attribute_list;

        let current: u64 = get_attr(&attrs, ATTR_TOTAL_STAKED)
            .ok_or(error!(StakingError::MissingCollectionStats))?
            .parse()
            .map_err(|_| error!(StakingError::InvalidAttribute))?;
        let next = if delta >= 0 {
            current.checked_add(delta as u64)
        } else {
            current.checked_sub(delta.unsigned_abs() as u64)
        }
        .ok_or(error!(StakingError::Overflow))?;
        set_attr(&mut attrs, ATTR_TOTAL_STAKED, next.to_string());

        UpdateCollectionPluginV1CpiBuilder::new(self.core_program)
            .collection(self.collection)
            .payer(self.payer)
            .authority(Some(self.update_authority))
            .system_program(self.system_program)
            .plugin(Plugin::Attributes(Attributes {
                attribute_list: attrs,
            }))
            .invoke_signed(signer_seeds)?;

        msg!("total_staked: {} -> {}", current, next);
        Ok(())
    }
}

/// Mint `amount` reward tokens, signed by the config PDA (the mint authority).
pub fn mint_rewards<'info>(
    token_program: &AccountInfo<'info>,
    mint: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    config: &AccountInfo<'info>,
    signer_seeds: &[&[&[u8]]],
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    mint_to(
        CpiContext::new_with_signer(
            token_program.key(),
            MintTo {
                mint: mint.clone(),
                to: to.clone(),
                authority: config.clone(),
            },
            signer_seeds,
        ),
        amount,
    )
}
