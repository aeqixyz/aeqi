//! Bit-flag access control. Direct port of EVM
//! `core/libraries/AclLibrary.sol`. Each module's `trust_acl: u64` carries
//! one bit per `AclFlag`; `has_acl(acl, flag)` does the same `(acl >> flag) & 1`
//! check the EVM `_hasPermission` helper does.

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum AclFlag {
    SetNumericConfig = 0,
    SetAddressConfig = 1,
    SetBytesConfig = 2,
    SetBooleanConfig = 3,
    ReplaceModule = 4,
    RemoveModule = 5,
    SetAclBetweenModules = 6,
    Execute = 7,
    Pause = 8,
    Unpause = 9,
    CreateTrust = 10,
    DeployModule = 11,
    TransferFunds = 12,
    ResetModules = 13,
}

#[inline]
pub fn has_acl(acl: u64, flag: AclFlag) -> bool {
    (acl >> (flag as u8)) & 1 == 1
}

#[inline]
pub fn add_acl(acl: u64, flag: AclFlag) -> u64 {
    acl | (1u64 << (flag as u8))
}

#[inline]
pub fn remove_acl(acl: u64, flag: AclFlag) -> u64 {
    acl & !(1u64 << (flag as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_round_trip() {
        let acl = 0u64;
        let acl = add_acl(acl, AclFlag::Execute);
        assert!(has_acl(acl, AclFlag::Execute));
        assert!(!has_acl(acl, AclFlag::Pause));
        let acl = add_acl(acl, AclFlag::Pause);
        assert!(has_acl(acl, AclFlag::Pause));
        let acl = remove_acl(acl, AclFlag::Execute);
        assert!(!has_acl(acl, AclFlag::Execute));
        assert!(has_acl(acl, AclFlag::Pause));
    }

    #[test]
    fn high_flags_dont_collide() {
        let acl = add_acl(0, AclFlag::ResetModules);
        assert!(has_acl(acl, AclFlag::ResetModules));
        // Bit 13 should NOT trigger any earlier flags.
        for f in [
            AclFlag::SetNumericConfig,
            AclFlag::Execute,
            AclFlag::Pause,
        ] {
            assert!(!has_acl(acl, f));
        }
    }
}
