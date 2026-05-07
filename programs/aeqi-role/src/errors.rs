use anchor_lang::prelude::*;

#[error_code]
pub enum AeqiRoleError {
    #[msg("caller does not hold a role with authority for this action")]
    Unauthorized,
    #[msg("role is not vacant")]
    RoleNotVacant,
    #[msg("role is not occupied")]
    RoleNotOccupied,
    #[msg("authority walk did not reach the target role")]
    AuthorityNotFound,
    #[msg("authority walk passed an account that did not match the expected parent")]
    InvalidAuthorityWalk,
    #[msg("authority walk exceeded the maximum depth")]
    AuthorityWalkTooDeep,
    #[msg("checkpoint slot is after the requested query slot")]
    CheckpointAfterQuery,
    #[msg("prev_checkpoint required when re-delegating away from a prior delegatee")]
    PrevCheckpointRequired,
}
