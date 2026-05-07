use anchor_lang::prelude::*;

/// Core TRUST account — one per AEQI company. PDA seeded `[b"trust", trust_id]`.
#[account]
#[derive(InitSpace)]
pub struct Trust {
    pub trust_id: [u8; 32],
    pub authority: Pubkey,
    pub creation_mode: bool,
    pub paused: bool,
    pub module_count: u32,
    pub bump: u8,
}

/// Per-module record under a TRUST. PDA seeded
/// `[b"module", trust, module_id]`. Holds the program ID that implements this
/// module slot (the Solana equivalent of EVM beacon-proxy implementation
/// pointers) and the bit-flag ACL for module → TRUST permissions.
#[account]
#[derive(InitSpace)]
pub struct Module {
    pub trust: Pubkey,
    pub module_id: [u8; 32],
    pub program_id: Pubkey,
    pub trust_acl: u64,
    pub initialized: u8,
    pub bump: u8,
}

#[repr(u8)]
pub enum ModuleInitState {
    Pending = 0,
    Initialized = 1,
    Finalized = 2,
}

/// Edge in the inter-module ACL graph. PDA seeded
/// `[b"acl_edge", trust, source_module_id, target_module_id]`. Mirrors the EVM
/// `Module.moduleAcls` mapping but keyed on module IDs rather than addresses
/// so the edge survives module program-id swaps.
#[account]
#[derive(InitSpace)]
pub struct ModuleAclEdge {
    pub trust: Pubkey,
    pub source_module_id: [u8; 32],
    pub target_module_id: [u8; 32],
    pub flags: u64,
    pub bump: u8,
}

/// Numeric config slot. PDA seeded `[b"cfg_num", trust, key]`.
#[account]
#[derive(InitSpace)]
pub struct NumericConfig {
    pub trust: Pubkey,
    pub key: [u8; 32],
    pub value: u128,
    pub bump: u8,
}

/// Address config slot. PDA seeded `[b"cfg_addr", trust, key]`.
#[account]
#[derive(InitSpace)]
pub struct AddressConfig {
    pub trust: Pubkey,
    pub key: [u8; 32],
    pub value: Pubkey,
    pub bump: u8,
}

/// Bytes config slot — used to carry borsh-serialized module config to
/// `finalize`. PDA seeded `[b"cfg_bytes", trust, key]`.
#[account]
pub struct BytesConfig {
    pub trust: Pubkey,
    pub key: [u8; 32],
    pub value: Vec<u8>,
    pub bump: u8,
}

impl BytesConfig {
    /// Fixed overhead (Pubkey + key + Vec length prefix + bump). Caller adds
    /// the value length on top.
    pub const INIT_SPACE_BASE: usize = 32 + 32 + 4 + 1;
}
