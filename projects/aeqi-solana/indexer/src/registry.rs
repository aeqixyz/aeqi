//! Anchor event discriminator → typed event name registry.
//!
//! Anchor encodes events as `Program data: base64(disc || borsh)` where
//! `disc = sha256("event:" + EventName)[..8]`. We pre-compute the
//! discriminators for every event our 7 programs emit so the live tail can
//! print typed names (and the DB sink can route to typed columns).

use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;

pub fn anchor_event_disc(name: &str) -> [u8; 8] {
    let preimage = format!("event:{}", name);
    let hash = Sha256::digest(preimage.as_bytes());
    let mut out = [0u8; 8];
    out.copy_from_slice(&hash[..8]);
    out
}

#[derive(Debug, Clone, Copy)]
pub struct EventMeta {
    pub program: &'static str,
    pub event: &'static str,
}

const EVENTS: &[(&str, &str, &[&str])] = &[
    (
        "aeqi_trust",
        "4CtmLZSLR3t1nKa3A2XD7F2awU5WajiNMxvHCiEDoBnD",
        &[
            "TrustInitialized",
            "TrustFinalized",
            "TrustPauseChanged",
            "ModuleRegistered",
            "ModuleAclSet",
        ],
    ),
    (
        "aeqi_factory",
        "4VvrC3pQ2hTUNJ7i5TnYzr9xnAU2P6T5ULwbbZnJES4T",
        &[
            "CompanyCreated",
            "CompanySpawned",
            "TemplateRegistered",
            "TemplateInstantiated",
        ],
    ),
    (
        "aeqi_role",
        "8KgcKNqW94Xonj5H3s64Cgf3NmDPMjhpL3PfxeFnDV1r",
        &[
            "RoleTypeCreated",
            "RoleCreated",
            "RoleAssigned",
            "RoleTransferred",
            "RoleResigned",
            "RoleDelegated",
        ],
    ),
    (
        "aeqi_governance",
        "dXXXHVt3y8PXdVtw9yGWSb67hiDa7YkyuUTfi6xRLen",
        &[
            "ConfigRegistered",
            "ProposalCreated",
            "VoteCast",
            "ProposalExecuted",
        ],
    ),
    (
        "aeqi_token",
        "28vYmAxQVZkqGwrH28gXDYNdWBPY7dW5odeUvoAHkw8r",
        &[
            "TokenModuleInitialized",
            "MintCreated",
            "TokensMinted",
            "TokensBurned",
        ],
    ),
    (
        "aeqi_treasury",
        "CQ7TGZFmkoZh61xgKnbjcj9Uomht38LqeihMNsY4p9KC",
        &["TreasuryWithdrew", "TreasuryDeposited"],
    ),
    (
        "aeqi_vesting",
        "24mJEeCHs492NGCJADvfb9zWDcqoDWNCpCYC2xAE2VBs",
        &["PositionCreated", "Claimed"],
    ),
];

pub static REGISTRY: Lazy<HashMap<(Pubkey, [u8; 8]), EventMeta>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for (program, pid, events) in EVENTS {
        let pk = Pubkey::from_str(pid).expect("hardcoded program id parses");
        for ev in events.iter() {
            m.insert(
                (pk, anchor_event_disc(ev)),
                EventMeta { program, event: ev },
            );
        }
    }
    m
});

pub fn lookup(program_id: &Pubkey, disc: &[u8]) -> Option<EventMeta> {
    if disc.len() < 8 {
        return None;
    }
    let mut key = [0u8; 8];
    key.copy_from_slice(&disc[..8]);
    REGISTRY.get(&(*program_id, key)).copied()
}

pub fn event_count() -> usize {
    REGISTRY.len()
}
