use anchor_lang::prelude::*;

#[error_code]
pub enum CapTableError {
    // -----------------------------------------------------------------------
    // Entity errors (6000–6009)
    // -----------------------------------------------------------------------
    #[msg("Entity name exceeds maximum length")]
    EntityNameTooLong,

    #[msg("Jurisdiction string exceeds maximum length")]
    JurisdictionTooLong,

    #[msg("Registration ID exceeds maximum length")]
    RegistrationIdTooLong,

    #[msg("Entity name cannot be empty")]
    EntityNameEmpty,

    // -----------------------------------------------------------------------
    // Share class errors (6010–6019)
    // -----------------------------------------------------------------------
    #[msg("Share class name exceeds maximum length")]
    ClassNameTooLong,

    #[msg("Share class name cannot be empty")]
    ClassNameEmpty,

    #[msg("Total authorized shares must be greater than zero")]
    ZeroAuthorizedShares,

    #[msg("Maximum number of share classes (255) reached")]
    MaxShareClassesReached,

    // -----------------------------------------------------------------------
    // Member errors (6020–6029)
    // -----------------------------------------------------------------------
    #[msg("Member is not in active status")]
    MemberNotActive,

    #[msg("Member is not KYC verified")]
    KycRequired,

    #[msg("Member does not have accredited investor status")]
    AccreditationRequired,

    // -----------------------------------------------------------------------
    // Share issuance errors (6030–6039)
    // -----------------------------------------------------------------------
    #[msg("Issuance would exceed total authorized shares for this class")]
    ExceedsAuthorizedShares,

    #[msg("Cannot issue zero shares")]
    ZeroShareIssuance,

    #[msg("Maximum holder count for this share class has been reached")]
    MaxHoldersReached,

    // -----------------------------------------------------------------------
    // Vesting errors (6040–6049)
    // -----------------------------------------------------------------------
    #[msg("Vesting start time must be before end time")]
    InvalidVestingPeriod,

    #[msg("Cliff time must be between start and end time")]
    InvalidCliffTime,

    #[msg("Vesting schedule has been revoked")]
    VestingRevoked,

    #[msg("No tokens available to claim at this time")]
    NothingToClaim,

    #[msg("Vesting amount must be greater than zero")]
    ZeroVestingAmount,

    #[msg("A vesting schedule already exists for this member and share class")]
    VestingAlreadyExists,

    // -----------------------------------------------------------------------
    // Transfer restriction errors (6050–6059)
    // -----------------------------------------------------------------------
    #[msg("Transfers are locked until the lock-up period ends")]
    LockupActive,

    #[msg("Right of first refusal has not been cleared for this transfer")]
    RofrNotCleared,

    #[msg("Transfers are not permitted for this share class")]
    TransfersNotPermitted,

    // -----------------------------------------------------------------------
    // Authority / access errors (6060–6069)
    // -----------------------------------------------------------------------
    #[msg("Signer is not the entity authority")]
    UnauthorizedAuthority,

    #[msg("The provided entity does not match the expected entity")]
    EntityMismatch,

    // -----------------------------------------------------------------------
    // Arithmetic / overflow errors (6070–6079)
    // -----------------------------------------------------------------------
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
}
