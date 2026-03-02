use anchor_lang::prelude::*;

#[error_code]
pub enum EntityLegalError {
    #[msg("Entity name exceeds maximum length")]
    EntityNameTooLong,

    #[msg("Entity name cannot be empty")]
    EntityNameEmpty,

    #[msg("Jurisdiction string exceeds maximum length")]
    JurisdictionTooLong,

    #[msg("Registration ID exceeds maximum length")]
    RegistrationIdTooLong,

    #[msg("Series name exceeds maximum length")]
    SeriesNameTooLong,

    #[msg("Series name cannot be empty")]
    SeriesNameEmpty,

    #[msg("Maximum series count reached")]
    MaxSeriesReached,

    #[msg("Member is not in active status")]
    MemberNotActive,

    #[msg("Member KYC verification is required for this operation")]
    KycRequired,

    #[msg("Member KYC has expired and must be renewed")]
    KycExpired,

    #[msg("Member is flagged as a restricted person")]
    RestrictedPerson,

    #[msg("Receiver would exceed the 25% anonymous holding threshold without KYC")]
    KycRequiredAbove25Pct,

    #[msg("Mint amount must be greater than zero")]
    ZeroMintAmount,

    #[msg("Swap amount must be greater than zero")]
    ZeroSwapAmount,

    #[msg("KYC is required to swap utility tokens to security tokens")]
    SwapRequiresKyc,

    #[msg("Proposal voting period has ended")]
    VotingPeriodEnded,

    #[msg("Proposal voting period has not ended yet")]
    VotingPeriodNotEnded,

    #[msg("Proposal is not in active status")]
    ProposalNotActive,

    #[msg("Member has already voted on this proposal")]
    AlreadyVoted,

    #[msg("Voter has zero utility tokens — no voting power")]
    NoVotingPower,

    #[msg("Signer is not the entity authority")]
    UnauthorizedAuthority,

    #[msg("Entity key mismatch")]
    EntityMismatch,

    #[msg("Security mint key mismatch")]
    SecurityMintMismatch,

    #[msg("Utility mint key mismatch")]
    UtilityMintMismatch,

    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,

    #[msg("Foundation authority required for this operation")]
    FoundationRequired,
}
