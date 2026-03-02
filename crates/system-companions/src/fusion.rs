use rand::Rng;
use chrono::Utc;

use crate::companion::{Companion, PersonaStatus, Rarity};
use crate::names;

#[derive(Debug)]
pub enum FusionError {
    RarityMismatch,
    AlreadyMaxRarity,
    SameCompanion,
    CompanionIsFamiliar,
    WrongCount,
    DuplicateCompanion,
}

impl std::fmt::Display for FusionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RarityMismatch => write!(f, "both companions must be the same rarity"),
            Self::AlreadyMaxRarity => write!(f, "SS companions cannot be fused further"),
            Self::SameCompanion => write!(f, "cannot fuse a companion with itself"),
            Self::CompanionIsFamiliar => write!(f, "cannot fuse the active familiar"),
            Self::WrongCount => write!(f, "fusion requires exactly 4 companions"),
            Self::DuplicateCompanion => write!(f, "all companions must be different"),
        }
    }
}

impl std::error::Error for FusionError {}

pub fn validate_fusion(a: &Companion, b: &Companion) -> Result<Rarity, FusionError> {
    if a.id == b.id {
        return Err(FusionError::SameCompanion);
    }
    if a.rarity != b.rarity {
        return Err(FusionError::RarityMismatch);
    }
    if a.rarity == Rarity::SS {
        return Err(FusionError::AlreadyMaxRarity);
    }
    if a.is_familiar || b.is_familiar {
        return Err(FusionError::CompanionIsFamiliar);
    }
    Ok(a.rarity.next().unwrap())
}

pub fn fuse(a: &Companion, b: &Companion) -> Result<Companion, FusionError> {
    let target_rarity = validate_fusion(a, b)?;
    let mut rng = rand::rng();

    let total_bond = a.bond_xp + b.bond_xp;
    let bond_inheritance_ratio = 0.25;
    let inherited_xp = (total_bond as f64 * bond_inheritance_ratio) as u64;

    let primary = if a.bond_xp >= b.bond_xp { a } else { b };
    let secondary = if a.bond_xp >= b.bond_xp { b } else { a };

    let archetype = if rng.random_bool(0.7) {
        primary.archetype
    } else {
        secondary.archetype
    };

    let dere_type = if rng.random_bool(0.6) {
        primary.dere_type
    } else {
        secondary.dere_type
    };

    let region = if rng.random_bool(0.5) {
        primary.region
    } else {
        secondary.region
    };

    let aesthetic = if rng.random_bool(0.5) {
        primary.aesthetic
    } else {
        secondary.aesthetic
    };

    let name = names::generate(&mut rng, &region);
    let personality_seed: u64 = primary.personality_seed.wrapping_mul(31).wrapping_add(secondary.personality_seed);

    // Blend anime inspirations: 2 from primary + 1 from secondary.
    let mut anime_inspirations = Vec::new();
    for insp in primary.anime_inspirations.iter().take(2) {
        anime_inspirations.push(insp.clone());
    }
    if let Some(insp) = secondary.anime_inspirations.first() {
        anime_inspirations.push(insp.clone());
    }

    // Inherit last name from primary if available; generate for A+.
    let last_name = if primary.last_name.is_some() {
        primary.last_name.clone()
    } else if target_rarity >= Rarity::A {
        Some(names::generate_last_name(&mut rng, &region))
    } else {
        None
    };

    // Generate title for S/SS.
    let title = if target_rarity >= Rarity::S {
        Some(names::generate_title(&mut rng, &archetype))
    } else {
        None
    };

    let mut result = Companion {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        archetype,
        dere_type,
        region,
        aesthetic,
        rarity: target_rarity,
        bond_level: 0,
        bond_xp: inherited_xp,
        is_familiar: false,
        familiar_eligible: false,
        created_at: Utc::now(),
        fused_from: Some([a.id.clone(), b.id.clone()]),
        personality_seed,
        anime_inspirations,
        persona_status: PersonaStatus::Pending,
        title,
        last_name,
    };

    let mut level_check = true;
    while level_check {
        level_check = result.add_bond_xp(0);
    }
    loop {
        let next = Companion::bond_xp_for_level(result.bond_level + 1);
        if result.bond_xp >= next {
            result.bond_level += 1;
        } else {
            break;
        }
    }

    if result.rarity >= Rarity::SS && result.bond_level >= 5 {
        result.familiar_eligible = true;
    }

    Ok(result)
}

/// Fuse 4 companions of any rarity. Result rarity is random, weighted by input quality.
/// Higher-rarity inputs improve odds of getting a higher-rarity result.
pub fn fuse_multi(companions: &[&Companion]) -> Result<Companion, FusionError> {
    if companions.len() != 4 {
        return Err(FusionError::WrongCount);
    }

    // Validate: all different IDs, none familiar
    for i in 0..companions.len() {
        if companions[i].is_familiar {
            return Err(FusionError::CompanionIsFamiliar);
        }
        for j in (i + 1)..companions.len() {
            if companions[i].id == companions[j].id {
                return Err(FusionError::DuplicateCompanion);
            }
        }
    }

    let mut rng = rand::rng();

    // Calculate fusion power from input rarities (C=1, B=2, A=3, S=4, SS=5)
    let rarity_value = |r: Rarity| -> u32 {
        match r { Rarity::C => 1, Rarity::B => 2, Rarity::A => 3, Rarity::S => 4, Rarity::SS => 5 }
    };
    let power: u32 = companions.iter().map(|c| rarity_value(c.rarity)).sum(); // 4-20
    let bonus = (power - 4) as f64; // 0-16

    // Weighted random rarity. Quadratic SS bonus so high inputs really matter.
    let w_c  = (300.0 - bonus * 18.0).max(5.0) as u32;
    let w_b  = (200.0 - bonus * 10.0).max(5.0) as u32;
    let w_a  = (80.0 + bonus * 2.0) as u32;
    let w_s  = (20.0 + bonus * 8.0) as u32;
    let w_ss = (1.0 + bonus * bonus * 0.5) as u32;

    let total = w_c + w_b + w_a + w_s + w_ss;
    let roll = rng.random_range(0..total);

    let target_rarity = if roll < w_c {
        Rarity::C
    } else if roll < w_c + w_b {
        Rarity::B
    } else if roll < w_c + w_b + w_a {
        Rarity::A
    } else if roll < w_c + w_b + w_a + w_s {
        Rarity::S
    } else {
        Rarity::SS
    };

    // Pick primary (highest bond) and secondary for trait inheritance
    let mut sorted: Vec<&Companion> = companions.to_vec();
    sorted.sort_by(|a, b| b.bond_xp.cmp(&a.bond_xp));
    let primary = sorted[0];
    let secondary = sorted[1];

    let total_bond: u64 = companions.iter().map(|c| c.bond_xp).sum();
    let inherited_xp = (total_bond as f64 * 0.2) as u64;

    let archetype = if rng.random_bool(0.6) { primary.archetype } else { secondary.archetype };
    let dere_type = if rng.random_bool(0.6) { primary.dere_type } else { secondary.dere_type };
    let region = if rng.random_bool(0.5) { primary.region } else { secondary.region };
    let aesthetic = if rng.random_bool(0.5) { primary.aesthetic } else { secondary.aesthetic };

    let name = names::generate(&mut rng, &region);
    let personality_seed: u64 = companions.iter()
        .fold(0u64, |acc, c| acc.wrapping_mul(31).wrapping_add(c.personality_seed));

    let mut anime_inspirations = Vec::new();
    for c in companions.iter().take(3) {
        if let Some(insp) = c.anime_inspirations.first() {
            anime_inspirations.push(insp.clone());
        }
    }

    let last_name = companions.iter()
        .find_map(|c| c.last_name.clone())
        .or_else(|| if target_rarity >= Rarity::A {
            Some(names::generate_last_name(&mut rng, &region))
        } else {
            None
        });

    let title = if target_rarity >= Rarity::S {
        Some(names::generate_title(&mut rng, &archetype))
    } else {
        None
    };

    let fused_ids: Vec<String> = companions.iter().map(|c| c.id.clone()).collect();

    let mut result = Companion {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        archetype,
        dere_type,
        region,
        aesthetic,
        rarity: target_rarity,
        bond_level: 0,
        bond_xp: inherited_xp,
        is_familiar: false,
        familiar_eligible: false,
        created_at: Utc::now(),
        fused_from: Some([
            fused_ids.first().cloned().unwrap_or_default(),
            fused_ids.get(1).cloned().unwrap_or_default(),
        ]),
        personality_seed,
        anime_inspirations,
        persona_status: PersonaStatus::Pending,
        title,
        last_name,
    };

    loop {
        let next = Companion::bond_xp_for_level(result.bond_level + 1);
        if result.bond_xp >= next {
            result.bond_level += 1;
        } else {
            break;
        }
    }

    if result.rarity >= Rarity::SS && result.bond_level >= 5 {
        result.familiar_eligible = true;
    }

    Ok(result)
}

pub fn fusion_preview_text(a: &Companion, b: &Companion) -> Result<String, FusionError> {
    let target = validate_fusion(a, b)?;
    Ok(format!(
        "Fusion Pipeline\n\
         \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n\
         {a_emoji} {a_name} ({a_rarity})\n\
         {b_emoji} {b_name} ({b_rarity})\n\
         \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n\
         \u{2192} Result: {target_emoji} {target} rarity\n\
         Hook inheritance: {bond}% of combined XP\n\n\
         \u{26A0} Both companions will be consumed.",
        a_emoji = a.rarity.color_emoji(),
        a_name = a.display_name(),
        a_rarity = a.rarity,
        b_emoji = b.rarity.color_emoji(),
        b_name = b.display_name(),
        b_rarity = b.rarity,
        target_emoji = target.color_emoji(),
        target = target,
        bond = 25,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gacha::{GachaEngine, PityState};

    fn make_companion(rarity: Rarity) -> Companion {
        let engine = GachaEngine::default();
        let mut pity = PityState::default();
        let mut c = engine.pull(&mut pity);
        c.rarity = rarity;
        c
    }

    #[test]
    fn test_basic_fusion() {
        let a = make_companion(Rarity::C);
        let b = make_companion(Rarity::C);
        let result = fuse(&a, &b).unwrap();
        assert_eq!(result.rarity, Rarity::B);
        assert!(result.fused_from.is_some());
    }

    #[test]
    fn test_fusion_rarity_mismatch() {
        let a = make_companion(Rarity::C);
        let b = make_companion(Rarity::B);
        assert!(fuse(&a, &b).is_err());
    }

    #[test]
    fn test_fusion_ss_blocked() {
        let a = make_companion(Rarity::SS);
        let b = make_companion(Rarity::SS);
        assert!(fuse(&a, &b).is_err());
    }

    #[test]
    fn test_fusion_same_companion() {
        let a = make_companion(Rarity::C);
        let b = a.clone();
        assert!(fuse(&a, &b).is_err());
    }

    #[test]
    fn test_bond_inheritance() {
        let mut a = make_companion(Rarity::B);
        let mut b = make_companion(Rarity::B);
        a.bond_xp = 1000;
        b.bond_xp = 500;
        let result = fuse(&a, &b).unwrap();
        assert!(result.bond_xp > 0);
        assert!(result.bond_xp <= 1500);
    }

    #[test]
    fn test_full_chain_c_to_ss() {
        let mut companions: Vec<Companion> = (0..16).map(|_| make_companion(Rarity::C)).collect();

        let mut b_tier: Vec<Companion> = Vec::new();
        while companions.len() >= 2 {
            let b = companions.pop().unwrap();
            let a = companions.pop().unwrap();
            b_tier.push(fuse(&a, &b).unwrap());
        }
        assert_eq!(b_tier.len(), 8);
        assert!(b_tier.iter().all(|c| c.rarity == Rarity::B));

        let mut a_tier: Vec<Companion> = Vec::new();
        while b_tier.len() >= 2 {
            let b = b_tier.pop().unwrap();
            let a = b_tier.pop().unwrap();
            a_tier.push(fuse(&a, &b).unwrap());
        }
        assert_eq!(a_tier.len(), 4);
        assert!(a_tier.iter().all(|c| c.rarity == Rarity::A));

        let mut s_tier: Vec<Companion> = Vec::new();
        while a_tier.len() >= 2 {
            let b = a_tier.pop().unwrap();
            let a = a_tier.pop().unwrap();
            s_tier.push(fuse(&a, &b).unwrap());
        }
        assert_eq!(s_tier.len(), 2);
        assert!(s_tier.iter().all(|c| c.rarity == Rarity::S));

        let ss = fuse(&s_tier[0], &s_tier[1]).unwrap();
        assert_eq!(ss.rarity, Rarity::SS);
    }

    #[test]
    fn test_fuse_multi_basic() {
        let a = make_companion(Rarity::C);
        let b = make_companion(Rarity::B);
        let c = make_companion(Rarity::A);
        let d = make_companion(Rarity::S);
        let result = fuse_multi(&[&a, &b, &c, &d]).unwrap();
        // Result should be some valid rarity
        assert!(matches!(result.rarity, Rarity::C | Rarity::B | Rarity::A | Rarity::S | Rarity::SS));
    }

    #[test]
    fn test_fuse_multi_wrong_count() {
        let a = make_companion(Rarity::C);
        let b = make_companion(Rarity::C);
        assert!(fuse_multi(&[&a, &b]).is_err());
    }

    #[test]
    fn test_fuse_multi_duplicate() {
        let a = make_companion(Rarity::C);
        let b = make_companion(Rarity::C);
        let c = make_companion(Rarity::C);
        assert!(fuse_multi(&[&a, &b, &c, &a]).is_err());
    }
}
