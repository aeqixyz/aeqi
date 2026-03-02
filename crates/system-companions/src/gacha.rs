use rand::Rng;
use chrono::Utc;

use crate::anime;
use crate::companion::{
    Aesthetic, Archetype, Companion, DereType, PersonaStatus, Rarity, Region,
};
use crate::names;

pub struct GachaRates {
    pub c: f64,
    pub b: f64,
    pub a: f64,
    pub s: f64,
    pub ss: f64,
}

impl Default for GachaRates {
    fn default() -> Self {
        Self {
            c: 0.50,
            b: 0.30,
            a: 0.15,
            s: 0.045,
            ss: 0.005,
        }
    }
}

#[derive(Default)]
pub struct PityState {
    pub pulls_since_s_or_above: u32,
    pub pulls_since_a_or_above: u32,
    pub total_pulls: u64,
}

impl PityState {
    pub const S_PITY_THRESHOLD: u32 = 50;
    pub const A_PITY_THRESHOLD: u32 = 20;

    pub fn adjust_rates(&self, base: &GachaRates) -> GachaRates {
        let mut rates = GachaRates {
            c: base.c,
            b: base.b,
            a: base.a,
            s: base.s,
            ss: base.ss,
        };

        if self.pulls_since_a_or_above >= Self::A_PITY_THRESHOLD {
            rates.a = 1.0;
            rates.c = 0.0;
            rates.b = 0.0;
            rates.s = 0.0;
            rates.ss = 0.0;
            return rates;
        }

        if self.pulls_since_s_or_above >= Self::S_PITY_THRESHOLD {
            rates.s = 0.90;
            rates.ss = 0.10;
            rates.c = 0.0;
            rates.b = 0.0;
            rates.a = 0.0;
            return rates;
        }

        if self.pulls_since_s_or_above > 30 {
            let bonus = (self.pulls_since_s_or_above - 30) as f64 * 0.02;
            rates.s += bonus * 0.9;
            rates.ss += bonus * 0.1;
            let total_bonus = bonus;
            rates.c -= total_bonus * 0.6;
            rates.b -= total_bonus * 0.4;
            rates.c = rates.c.max(0.0);
            rates.b = rates.b.max(0.0);
        }

        rates
    }

    pub fn record_pull(&mut self, rarity: Rarity) {
        self.total_pulls += 1;
        match rarity {
            Rarity::SS | Rarity::S => {
                self.pulls_since_s_or_above = 0;
                self.pulls_since_a_or_above = 0;
            }
            Rarity::A => {
                self.pulls_since_s_or_above += 1;
                self.pulls_since_a_or_above = 0;
            }
            _ => {
                self.pulls_since_s_or_above += 1;
                self.pulls_since_a_or_above += 1;
            }
        }
    }
}

#[derive(Default)]
pub struct GachaEngine {
    pub rates: GachaRates,
}

impl GachaEngine {
    pub fn pull(&self, pity: &mut PityState) -> Companion {
        let mut rng = rand::rng();
        let effective_rates = pity.adjust_rates(&self.rates);
        let rarity = Self::roll_rarity(&mut rng, &effective_rates);
        let companion = Self::generate_companion(&mut rng, rarity);
        pity.record_pull(rarity);
        companion
    }

    pub fn pull_multi(&self, pity: &mut PityState, count: u32) -> Vec<Companion> {
        (0..count).map(|_| self.pull(pity)).collect()
    }

    fn roll_rarity(rng: &mut impl Rng, rates: &GachaRates) -> Rarity {
        let roll: f64 = rng.random();
        let mut cumulative = 0.0;

        cumulative += rates.ss;
        if roll < cumulative {
            return Rarity::SS;
        }
        cumulative += rates.s;
        if roll < cumulative {
            return Rarity::S;
        }
        cumulative += rates.a;
        if roll < cumulative {
            return Rarity::A;
        }
        cumulative += rates.b;
        if roll < cumulative {
            return Rarity::B;
        }
        Rarity::C
    }

    fn generate_companion(rng: &mut impl Rng, rarity: Rarity) -> Companion {
        let archetype = Archetype::ALL[rng.random_range(0..Archetype::ALL.len())];
        let dere_type = DereType::ALL[rng.random_range(0..DereType::ALL.len())];
        let region = Region::ALL[rng.random_range(0..Region::ALL.len())];
        let aesthetic = Aesthetic::ALL[rng.random_range(0..Aesthetic::ALL.len())];
        let personality_seed: u64 = rng.random();
        let name = names::generate(rng, &region);
        let anime_inspirations = anime::pick_inspirations(rng, rarity);

        // A+ get last names, S/SS get titles.
        let last_name = if rarity >= Rarity::A {
            Some(names::generate_last_name(rng, &region))
        } else {
            None
        };
        let title = if rarity >= Rarity::S {
            Some(names::generate_title(rng, &archetype))
        } else {
            None
        };

        Companion {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            archetype,
            dere_type,
            region,
            aesthetic,
            rarity,
            bond_level: 0,
            bond_xp: 0,
            is_familiar: false,
            familiar_eligible: false,
            created_at: Utc::now(),
            fused_from: None,
            personality_seed,
            anime_inspirations,
            persona_status: PersonaStatus::Pending,
            title,
            last_name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_rates_sum_to_one() {
        let rates = GachaRates::default();
        let total = rates.c + rates.b + rates.a + rates.s + rates.ss;
        assert!((total - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_pity_guarantees_a_at_threshold() {
        let mut pity = PityState::default();
        pity.pulls_since_a_or_above = PityState::A_PITY_THRESHOLD;
        let rates = pity.adjust_rates(&GachaRates::default());
        assert_eq!(rates.a, 1.0);
    }

    #[test]
    fn test_pity_guarantees_s_at_threshold() {
        let mut pity = PityState::default();
        pity.pulls_since_s_or_above = PityState::S_PITY_THRESHOLD;
        let rates = pity.adjust_rates(&GachaRates::default());
        assert!(rates.s + rates.ss >= 0.99);
    }

    #[test]
    fn test_pull_produces_valid_companion() {
        let engine = GachaEngine::default();
        let mut pity = PityState::default();
        let companion = engine.pull(&mut pity);
        assert!(!companion.id.is_empty());
        assert!(!companion.name.is_empty());
        assert_eq!(companion.bond_level, 0);
        assert_eq!(pity.total_pulls, 1);
    }

    #[test]
    fn test_multi_pull() {
        let engine = GachaEngine::default();
        let mut pity = PityState::default();
        let companions = engine.pull_multi(&mut pity, 10);
        assert_eq!(companions.len(), 10);
        assert_eq!(pity.total_pulls, 10);
    }

    #[test]
    fn test_rarity_distribution_reasonable() {
        let engine = GachaEngine::default();
        let mut pity = PityState::default();
        let mut counts = [0u32; 5];
        for _ in 0..10000 {
            let c = engine.pull(&mut pity);
            match c.rarity {
                Rarity::C => counts[0] += 1,
                Rarity::B => counts[1] += 1,
                Rarity::A => counts[2] += 1,
                Rarity::S => counts[3] += 1,
                Rarity::SS => counts[4] += 1,
            }
            pity = PityState::default();
        }
        assert!(counts[0] > 4000, "C should be ~50%: got {}", counts[0]);
        assert!(counts[1] > 2000, "B should be ~30%: got {}", counts[1]);
        assert!(counts[2] > 1000, "A should be ~15%: got {}", counts[2]);
        assert!(counts[3] > 200, "S should be ~4.5%: got {}", counts[3]);
    }
}
