use anchor_lang::prelude::*;

#[error_code]
pub enum StakingError {
    #[msg("Signer does not own this asset")]
    NotOwner,
    #[msg("Asset does not belong to this collection")]
    WrongCollection,
    #[msg("Asset is already staked")]
    AlreadyStaked,
    #[msg("Asset is not staked")]
    NotStaked,
    #[msg("Freeze period has not elapsed yet")]
    FreezePeriodNotElapsed,
    #[msg("Collection is missing the total_staked attribute")]
    MissingCollectionStats,
    #[msg("Could not parse a Core attribute value")]
    InvalidAttribute,
    #[msg("Could not deserialize a Core account")]
    InvalidCoreAccount,
    #[msg("Arithmetic overflow")]
    Overflow,
}
