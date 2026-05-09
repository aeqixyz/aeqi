use anchor_lang::prelude::*;

/// Template registered on-chain. PDA seeded `[b"template", template_id]`.
/// Mirrors EVM `FactoryLibrary.Template` — declares the module set, ACL graph,
/// and default value configs that `instantiate_template` will replay against
/// every fresh TRUST.
#[account]
pub struct Template {
    pub template_id: [u8; 32],
    pub admin: Pubkey,
    pub modules: Vec<ModuleSpec>,
    pub acl_edges: Vec<AclEdgeSpec>,
    pub bump: u8,
}

/// Module slot declaration in a template. The `program_id` points at a
/// concrete module program (aeqi_role, aeqi_token, aeqi_governance, …) and
/// `trust_acl` is the bit-flag mask granted to the module.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct ModuleSpec {
    pub module_id: [u8; 32],
    pub program_id: Pubkey,
    pub trust_acl: u64,
}

/// Inter-module ACL edge declaration. After all modules are deployed the
/// factory walks this list and CPIs `aeqi_trust::set_module_acl` per edge.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct AclEdgeSpec {
    pub source_module_id: [u8; 32],
    pub target_module_id: [u8; 32],
    pub flags: u64,
}

impl Template {
    /// Conservative size budget. 32 (template_id) + 32 (admin) + 4 (Vec len)
    /// + N * ModuleSpec::INIT_SPACE + 4 + M * AclEdgeSpec::INIT_SPACE + 1.
    pub fn space(num_modules: usize, num_edges: usize) -> usize {
        8  // discriminator
        + 32 // template_id
        + 32 // admin
        + 4 + num_modules * ModuleSpec::INIT_SPACE
        + 4 + num_edges * AclEdgeSpec::INIT_SPACE
        + 1  // bump
    }
}
