use anchor_lang::prelude::*;

#[error_code]
pub enum AeqiTrustError {
    #[msg("caller is not authorized for this trust")]
    Unauthorized,
    #[msg("denied — caller does not hold the required ACL flag")]
    DeniedAccess,
    #[msg("trust is paused")]
    TrustPaused,
    #[msg("operation is only permitted in creation mode")]
    NotInCreationMode,
    #[msg("trust has already been finalized")]
    AlreadyFinalized,
    #[msg("module has already been initialized")]
    ModuleAlreadyInitialized,
    #[msg("module has not yet been initialized")]
    ModuleNotInitialized,
    #[msg("config payload exceeds maximum size")]
    ConfigTooLarge,
}
