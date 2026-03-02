use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::companion::{Archetype, Companion, DereType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRelationship {
    /// Companion name (lexicographic: a < b for consistent ordering).
    pub agent_a: String,
    pub agent_b: String,
    /// Competence recognition: -1.0 to 1.0
    pub respect: f32,
    /// Personal like/dislike: -1.0 to 1.0
    pub affinity: f32,
    /// Reliability — starts at 0, earned: -1.0 to 1.0
    pub trust: f32,
    /// Competitive tension: 0.0 to 1.0
    pub rivalry: f32,
    /// How well they work together: -1.0 to 1.0
    pub synergy: f32,
    pub updated_at: DateTime<Utc>,
}

impl AgentRelationship {
    /// Clamp all values to valid ranges.
    pub fn clamp(&mut self) {
        self.respect = self.respect.clamp(-1.0, 1.0);
        self.affinity = self.affinity.clamp(-1.0, 1.0);
        self.trust = self.trust.clamp(-1.0, 1.0);
        self.rivalry = self.rivalry.clamp(0.0, 1.0);
        self.synergy = self.synergy.clamp(-1.0, 1.0);
    }

    /// Overall compatibility score (-1.0 to 1.0).
    pub fn overall_compatibility(&self) -> f32 {
        let score = (self.respect * 0.2)
            + (self.affinity * 0.3)
            + (self.trust * 0.2)
            + (self.synergy * 0.2)
            - (self.rivalry * 0.1);
        score.clamp(-1.0, 1.0)
    }

    /// Human-readable relationship label.
    pub fn relationship_label(&self) -> &'static str {
        let compat = self.overall_compatibility();
        if compat > 0.7 {
            "Soulbound"
        } else if compat > 0.4 {
            "Close Allies"
        } else if compat > 0.1 {
            "Cordial"
        } else if compat > -0.2 {
            "Tense"
        } else if compat > -0.5 {
            "Rivals"
        } else {
            "Hostile"
        }
    }

    /// Ensure canonical ordering (a < b).
    pub fn canonical_key(name_a: &str, name_b: &str) -> (String, String) {
        if name_a <= name_b {
            (name_a.to_string(), name_b.to_string())
        } else {
            (name_b.to_string(), name_a.to_string())
        }
    }
}

/// Archetype affinity matrix — returns (respect, affinity, synergy, rivalry) modifiers.
fn archetype_affinity(a: Archetype, b: Archetype) -> (f32, f32, f32, f32) {
    use Archetype::*;
    match (a, b) {
        // High synergy pairs.
        (Guardian, Healer) | (Healer, Guardian) => (0.3, 0.3, 0.5, 0.0),
        (Strategist, Builder) | (Builder, Strategist) => (0.3, 0.1, 0.4, 0.1),
        (Librarian, Archivist) | (Archivist, Librarian) => (0.4, 0.3, 0.5, 0.0),
        (Muse, Builder) | (Builder, Muse) => (0.2, 0.2, 0.3, 0.1),

        // Rivalry + synergy pairs.
        (Strategist, Trickster) | (Trickster, Strategist) => (0.2, 0.0, 0.3, 0.4),
        (Guardian, Trickster) | (Trickster, Guardian) => (-0.1, -0.1, 0.1, 0.3),

        // Tension pairs.
        (Muse, Archivist) | (Archivist, Muse) => (0.1, -0.1, 0.0, 0.2),
        (Healer, Trickster) | (Trickster, Healer) => (0.0, 0.1, 0.1, 0.2),

        // Same archetype — high respect, some rivalry.
        (a, b) if a == b => (0.3, 0.2, 0.2, 0.3),

        // Default: mild positive.
        _ => (0.1, 0.1, 0.1, 0.1),
    }
}

/// Dere-type interaction modifiers — returns (affinity_mod, rivalry_mod).
fn dere_interaction(a: DereType, b: DereType) -> (f32, f32) {
    use DereType::*;
    match (a, b) {
        // Tsundere + Deredere = classic affinity boost.
        (Tsundere, Deredere) | (Deredere, Tsundere) => (0.3, 0.0),
        // Kuudere + Genki = opposites attract.
        (Kuudere, Genki) | (Genki, Kuudere) => (0.2, 0.1),
        // Dandere + Oneesama = mentor/protege.
        (Dandere, Oneesama) | (Oneesama, Dandere) => (0.2, 0.0),
        // Yandere + anyone = rivalry boost.
        (Yandere, _) | (_, Yandere) => (0.0, 0.3),
        // Same dere type = understanding but competition.
        (a, b) if a == b => (0.15, 0.15),
        // Tsundere + Kuudere = mutual respect.
        (Tsundere, Kuudere) | (Kuudere, Tsundere) => (0.1, 0.1),
        _ => (0.0, 0.0),
    }
}

/// Seed an initial relationship from two companions' traits.
pub fn seed_from_traits(a: &Companion, b: &Companion) -> AgentRelationship {
    let (key_a, key_b) = AgentRelationship::canonical_key(&a.name, &b.name);

    let (arch_respect, arch_affinity, arch_synergy, arch_rivalry) =
        archetype_affinity(a.archetype, b.archetype);
    let (dere_affinity, dere_rivalry) = dere_interaction(a.dere_type, b.dere_type);

    // Same region bonus.
    let region_bonus = if a.region == b.region { 0.15 } else { 0.0 };

    let mut rel = AgentRelationship {
        agent_a: key_a,
        agent_b: key_b,
        respect: arch_respect,
        affinity: arch_affinity + dere_affinity + region_bonus,
        trust: 0.0, // trust is always earned
        rivalry: arch_rivalry + dere_rivalry,
        synergy: arch_synergy,
        updated_at: Utc::now(),
    };
    rel.clamp();
    rel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gacha::{GachaEngine, PityState};

    fn make_companion_with(archetype: Archetype, dere: DereType, region: crate::companion::Region) -> Companion {
        let engine = GachaEngine::default();
        let mut pity = PityState::default();
        let mut c = engine.pull(&mut pity);
        c.archetype = archetype;
        c.dere_type = dere;
        c.region = region;
        c
    }

    #[test]
    fn test_relationship_values_in_range() {
        let a = make_companion_with(Archetype::Guardian, DereType::Tsundere, crate::companion::Region::Tokyo);
        let b = make_companion_with(Archetype::Healer, DereType::Deredere, crate::companion::Region::Tokyo);
        let rel = seed_from_traits(&a, &b);

        assert!((-1.0..=1.0).contains(&rel.respect), "respect out of range: {}", rel.respect);
        assert!((-1.0..=1.0).contains(&rel.affinity), "affinity out of range: {}", rel.affinity);
        assert!((-1.0..=1.0).contains(&rel.trust), "trust out of range: {}", rel.trust);
        assert!((0.0..=1.0).contains(&rel.rivalry), "rivalry out of range: {}", rel.rivalry);
        assert!((-1.0..=1.0).contains(&rel.synergy), "synergy out of range: {}", rel.synergy);
    }

    #[test]
    fn test_relationship_symmetry() {
        let a = make_companion_with(Archetype::Strategist, DereType::Kuudere, crate::companion::Region::Osaka);
        let b = make_companion_with(Archetype::Trickster, DereType::Genki, crate::companion::Region::Kyoto);

        let rel_ab = seed_from_traits(&a, &b);
        let rel_ba = seed_from_traits(&b, &a);

        // Canonical keys should be the same regardless of order.
        assert_eq!(rel_ab.agent_a, rel_ba.agent_a);
        assert_eq!(rel_ab.agent_b, rel_ba.agent_b);

        // Values should be the same.
        assert!((rel_ab.respect - rel_ba.respect).abs() < 0.01);
        assert!((rel_ab.affinity - rel_ba.affinity).abs() < 0.01);
        assert!((rel_ab.synergy - rel_ba.synergy).abs() < 0.01);
    }

    #[test]
    fn test_same_region_bonus() {
        let a = make_companion_with(Archetype::Builder, DereType::Genki, crate::companion::Region::Tokyo);
        let mut b_same = make_companion_with(Archetype::Muse, DereType::Dandere, crate::companion::Region::Tokyo);
        let mut b_diff = make_companion_with(Archetype::Muse, DereType::Dandere, crate::companion::Region::Osaka);
        b_same.archetype = Archetype::Muse;
        b_same.dere_type = DereType::Dandere;
        b_diff.archetype = Archetype::Muse;
        b_diff.dere_type = DereType::Dandere;

        let rel_same = seed_from_traits(&a, &b_same);
        let rel_diff = seed_from_traits(&a, &b_diff);

        assert!(rel_same.affinity > rel_diff.affinity, "same region should have higher affinity");
    }

    #[test]
    fn test_guardian_healer_high_synergy() {
        let a = make_companion_with(Archetype::Guardian, DereType::Deredere, crate::companion::Region::Hokkaido);
        let b = make_companion_with(Archetype::Healer, DereType::Dandere, crate::companion::Region::Okinawa);
        let rel = seed_from_traits(&a, &b);

        assert!(rel.synergy >= 0.4, "Guardian+Healer should have high synergy: {}", rel.synergy);
    }

    #[test]
    fn test_yandere_boost_rivalry() {
        let a = make_companion_with(Archetype::Muse, DereType::Yandere, crate::companion::Region::Harajuku);
        let b = make_companion_with(Archetype::Builder, DereType::Genki, crate::companion::Region::Sapporo);
        let rel = seed_from_traits(&a, &b);

        assert!(rel.rivalry >= 0.3, "Yandere should boost rivalry: {}", rel.rivalry);
    }

    #[test]
    fn test_relationship_labels() {
        let mut rel = AgentRelationship {
            agent_a: "A".to_string(),
            agent_b: "B".to_string(),
            respect: 0.8,
            affinity: 0.9,
            trust: 0.7,
            rivalry: 0.0,
            synergy: 0.8,
            updated_at: Utc::now(),
        };
        assert_eq!(rel.relationship_label(), "Soulbound");

        rel.respect = -0.5;
        rel.affinity = -0.8;
        rel.trust = -0.5;
        rel.synergy = -0.7;
        rel.rivalry = 0.8;
        assert_eq!(rel.relationship_label(), "Hostile");
    }

    #[test]
    fn test_overall_compatibility_range() {
        let a = make_companion_with(Archetype::Librarian, DereType::Oneesama, crate::companion::Region::Kyoto);
        let b = make_companion_with(Archetype::Archivist, DereType::Dandere, crate::companion::Region::Kyoto);
        let rel = seed_from_traits(&a, &b);

        let compat = rel.overall_compatibility();
        assert!((-1.0..=1.0).contains(&compat), "compatibility out of range: {compat}");
    }
}
