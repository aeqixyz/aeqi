use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct RoleModuleState {
    pub trust: Pubkey,
    pub initialized: bool,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct RoleTypeConfig {
    pub vesting: bool,
    pub vesting_cliff: i64,
    pub vesting_duration: i64,
    pub fdv: bool,
    pub fdv_start: u128,
    pub fdv_end: u128,
    pub probationary_period: i64,
    pub severance_period: i64,
    pub contribution: bool,
}

#[account]
#[derive(InitSpace)]
pub struct RoleType {
    pub trust: Pubkey,
    pub role_type_id: [u8; 32],
    pub hierarchy: u32,
    pub config: RoleTypeConfig,
    pub role_count: u32,
    pub bump: u8,
}

#[repr(u8)]
pub enum RoleStatus {
    Vacant = 0,
    Occupied = 1,
    Resigned = 2,
    Removed = 3,
}

#[account]
#[derive(InitSpace)]
pub struct Role {
    pub trust: Pubkey,
    pub role_id: [u8; 32],
    pub role_type_id: [u8; 32],
    pub account: Pubkey,
    pub parent_role_id: [u8; 32],
    pub status: u8,
    pub status_since: i64,
    pub ipfs_cid: [u8; 64],
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct RoleDelegation {
    pub trust: Pubkey,
    pub role_id: [u8; 32],
    pub delegatee: Pubkey,
    pub bump: u8,
}

/// One per (account, role_type) pair. Updated on every assignment / delegation
/// change. `slot` records when this checkpoint was written; governance reads
/// it via `get_past_role_votes` requiring `ckpt.slot <= query_slot`.
#[account]
#[derive(InitSpace)]
pub struct RoleVoteCheckpoint {
    pub account: Pubkey,
    pub role_type_id: [u8; 32],
    pub slot: u64,
    pub count: u64,
    pub bump: u8,
}
