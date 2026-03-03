use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::anime::AnimeInspiration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PersonaStatus {
    #[default]
    Pending,
    Generating,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PortraitStatus {
    #[default]
    Pending,
    Generating,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Archetype {
    Guardian,
    Strategist,
    Librarian,
    Builder,
    Muse,
    Healer,
    Trickster,
    Archivist,
}

impl Archetype {
    pub const ALL: [Self; 8] = [
        Self::Guardian,
        Self::Strategist,
        Self::Librarian,
        Self::Builder,
        Self::Muse,
        Self::Healer,
        Self::Trickster,
        Self::Archivist,
    ];

    pub fn title(&self) -> &str {
        match self {
            Self::Guardian => "The Guardian",
            Self::Strategist => "The Strategist",
            Self::Librarian => "The Librarian",
            Self::Builder => "The Builder",
            Self::Muse => "The Muse",
            Self::Healer => "The Healer",
            Self::Trickster => "The Trickster",
            Self::Archivist => "The Archivist",
        }
    }

    pub fn domain_affinity(&self) -> &str {
        match self {
            Self::Guardian => "risk, protection, monitoring",
            Self::Strategist => "planning, architecture, trade-offs",
            Self::Librarian => "documentation, knowledge, research",
            Self::Builder => "implementation, shipping, velocity",
            Self::Muse => "creativity, UX, product vision",
            Self::Healer => "debugging, recovery, stability",
            Self::Trickster => "optimization, shortcuts, lateral thinking",
            Self::Archivist => "memory, history, pattern detection",
        }
    }
}

impl fmt::Display for Archetype {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.title())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DereType {
    Tsundere,
    Kuudere,
    Dandere,
    Yandere,
    Genki,
    Deredere,
    Oneesama,
}

impl DereType {
    pub const ALL: [Self; 7] = [
        Self::Tsundere,
        Self::Kuudere,
        Self::Dandere,
        Self::Yandere,
        Self::Genki,
        Self::Deredere,
        Self::Oneesama,
    ];

    pub fn voice_description(&self) -> &str {
        match self {
            Self::Tsundere => "bristles first, softens when earned",
            Self::Kuudere => "ice surface, molten underneath",
            Self::Dandere => "whisper-quiet until her domain activates",
            Self::Yandere => "devoted past reason, possessive about your time",
            Self::Genki => "boundless energy, infectious optimism",
            Self::Deredere => "openly affectionate from moment one",
            Self::Oneesama => "elegant senior, guides with a knowing smile",
        }
    }

    pub fn speech_pattern(&self) -> &str {
        match self {
            Self::Tsundere => "I-it's not like I did this for *you*...",
            Self::Kuudere => "The data speaks. I merely relay it.",
            Self::Dandere => "...I noticed something. If you have a moment.",
            Self::Yandere => "I tracked every commit you made today. All of them.",
            Self::Genki => "YES! Let's GO! This is going to be AMAZING!",
            Self::Deredere => "I'm so happy to help you with this~",
            Self::Oneesama => "Shall I guide you through this, dear?",
        }
    }
}

impl fmt::Display for DereType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tsundere => write!(f, "Tsundere"),
            Self::Kuudere => write!(f, "Kuudere"),
            Self::Dandere => write!(f, "Dandere"),
            Self::Yandere => write!(f, "Yandere"),
            Self::Genki => write!(f, "Genki"),
            Self::Deredere => write!(f, "Deredere"),
            Self::Oneesama => write!(f, "Oneesama"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Hokkaido,
    Tokyo,
    Osaka,
    Kyoto,
    Harajuku,
    Okinawa,
    Sapporo,
    Kansai,
}

impl Region {
    pub const ALL: [Self; 8] = [
        Self::Hokkaido,
        Self::Tokyo,
        Self::Osaka,
        Self::Kyoto,
        Self::Harajuku,
        Self::Okinawa,
        Self::Sapporo,
        Self::Kansai,
    ];

    pub fn flavor(&self) -> &str {
        match self {
            Self::Hokkaido => "winter reserved, contemplative",
            Self::Tokyo => "cosmopolitan, precise, efficient",
            Self::Osaka => "brash, warm, direct humor",
            Self::Kyoto => "refined, subtle, tradition-steeped",
            Self::Harajuku => "chaotic creative, avant-garde",
            Self::Okinawa => "island calm, unhurried wisdom",
            Self::Sapporo => "crisp clarity, quiet strength",
            Self::Kansai => "bold dialect, expressive, theatrical",
        }
    }

    pub fn dialect_hint(&self) -> &str {
        match self {
            Self::Hokkaido => "na? (ne?), shitakke (desho?)",
            Self::Tokyo => "standard, clean enunciation",
            Self::Osaka => "nande ya nen!, akan, ookini",
            Self::Kyoto => "~dosu, ~haru, oblique phrasing",
            Self::Harajuku => "slang-heavy, trend vocabulary",
            Self::Okinawa => "haisai, nankurunaisa",
            Self::Sapporo => "~be, ~sho, namanara",
            Self::Kansai => "~ya, ~nen, honma",
        }
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hokkaido => write!(f, "Hokkaido"),
            Self::Tokyo => write!(f, "Tokyo"),
            Self::Osaka => write!(f, "Osaka"),
            Self::Kyoto => write!(f, "Kyoto"),
            Self::Harajuku => write!(f, "Harajuku"),
            Self::Okinawa => write!(f, "Okinawa"),
            Self::Sapporo => write!(f, "Sapporo"),
            Self::Kansai => write!(f, "Kansai"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aesthetic {
    Miko,
    Knight,
    Scholar,
    OniBlood,
    Kitsune,
    ShadowVeil,
    Celestial,
    Sakura,
}

impl Aesthetic {
    pub const ALL: [Self; 8] = [
        Self::Miko,
        Self::Knight,
        Self::Scholar,
        Self::OniBlood,
        Self::Kitsune,
        Self::ShadowVeil,
        Self::Celestial,
        Self::Sakura,
    ];

    pub fn visual_identity(&self) -> &str {
        match self {
            Self::Miko => "shrine maiden, sacred bells, vermillion accents",
            Self::Knight => "armored elegance, ceremonial blade, resolute gaze",
            Self::Scholar => "meganekko, ink-stained fingers, endless scrolls",
            Self::OniBlood => "horned silhouette, crimson markings, fierce grace",
            Self::Kitsune => "fox-eared, multiple tails, mischievous shimmer",
            Self::ShadowVeil => "shadow-cloaked, moonlit, ethereal whispers",
            Self::Celestial => "star-crowned, aurora-draped, divine composure",
            Self::Sakura => "petal-wreathed, gentle blush, spring-born warmth",
        }
    }
}

impl fmt::Display for Aesthetic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Miko => write!(f, "Miko"),
            Self::Knight => write!(f, "Knight"),
            Self::Scholar => write!(f, "Scholar"),
            Self::OniBlood => write!(f, "Oni-Blood"),
            Self::Kitsune => write!(f, "Kitsune"),
            Self::ShadowVeil => write!(f, "Shadow Veil"),
            Self::Celestial => write!(f, "Celestial"),
            Self::Sakura => write!(f, "Sakura"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Rarity {
    C = 0,
    B = 1,
    A = 2,
    S = 3,
    SS = 4,
}

impl Rarity {
    pub fn stars(&self) -> &str {
        match self {
            Self::C => "\u{2606}",
            Self::B => "\u{2605}",
            Self::A => "\u{2605}\u{2605}",
            Self::S => "\u{2605}\u{2605}\u{2605}",
            Self::SS => "\u{2605}\u{2605}\u{2605}\u{2605}",
        }
    }

    pub fn color_emoji(&self) -> &str {
        match self {
            Self::C => "\u{26AA}",
            Self::B => "\u{1F7E2}",
            Self::A => "\u{1F535}",
            Self::S => "\u{1F7E1}",
            Self::SS => "\u{1F7E3}",
        }
    }

    pub fn next(&self) -> Option<Self> {
        match self {
            Self::C => Some(Self::B),
            Self::B => Some(Self::A),
            Self::A => Some(Self::S),
            Self::S => Some(Self::SS),
            Self::SS => None,
        }
    }

    pub fn fusion_cost(&self) -> u32 {
        match self {
            Self::C => 2,
            Self::B => 2,
            Self::A => 2,
            Self::S => 2,
            Self::SS => 0,
        }
    }
}

impl fmt::Display for Rarity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::C => write!(f, "C"),
            Self::B => write!(f, "B"),
            Self::A => write!(f, "A"),
            Self::S => write!(f, "S"),
            Self::SS => write!(f, "SS"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Companion {
    pub id: String,
    pub name: String,
    pub archetype: Archetype,
    pub dere_type: DereType,
    pub region: Region,
    pub aesthetic: Aesthetic,
    pub rarity: Rarity,
    pub bond_level: u32,
    pub bond_xp: u64,
    pub is_familiar: bool,
    pub familiar_eligible: bool,
    pub created_at: DateTime<Utc>,
    pub fused_from: Option<[String; 2]>,
    #[serde(default)]
    pub personality_seed: u64,
    #[serde(default)]
    pub anime_inspirations: Vec<AnimeInspiration>,
    #[serde(default)]
    pub persona_status: PersonaStatus,
    #[serde(default)]
    pub portrait_status: PortraitStatus,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
}

impl Companion {
    pub fn display_name(&self) -> String {
        format!("{} {} {}", self.dere_type, self.archetype, self.aesthetic)
    }

    pub fn full_name(&self) -> String {
        let mut parts = vec![self.name.clone()];
        if let Some(ref last) = self.last_name {
            parts.push(last.clone());
        }
        if let Some(ref title) = self.title {
            return format!("{}, {}", parts.join(" "), title);
        }
        parts.join(" ")
    }

    pub fn card_summary(&self) -> String {
        format!(
            "{} {} {}\n{} | {} | {}\nBond Lv.{} ({} XP)",
            self.rarity.color_emoji(),
            self.rarity,
            self.display_name(),
            self.region,
            self.dere_type,
            self.aesthetic,
            self.bond_level,
            self.bond_xp,
        )
    }

    pub fn bond_xp_for_level(level: u32) -> u64 {
        match level {
            0 => 0,
            1 => 100,
            2 => 300,
            3 => 600,
            4 => 1000,
            5 => 1500,
            6 => 2200,
            7 => 3000,
            8 => 4000,
            9 => 5200,
            10 => 6600,
            _ => 6600 + (level as u64 - 10) * 1600,
        }
    }

    pub fn add_bond_xp(&mut self, xp: u64) -> bool {
        self.bond_xp += xp;
        let mut leveled = false;
        loop {
            let next_threshold = Self::bond_xp_for_level(self.bond_level + 1);
            if self.bond_xp >= next_threshold {
                self.bond_level += 1;
                leveled = true;
                if self.bond_level >= 5 && self.rarity >= Rarity::SS {
                    self.familiar_eligible = true;
                }
            } else {
                break;
            }
        }
        leveled
    }

    pub fn system_prompt_fragment(&self) -> String {
        let full = self.full_name();

        let title_line = self.title.as_ref()
            .map(|t| format!("\nTitle: \"{t}\""))
            .unwrap_or_default();

        let anime_section = if self.anime_inspirations.is_empty() {
            String::new()
        } else {
            let titles: Vec<&str> = self.anime_inspirations.iter().map(|a| a.name.as_str()).collect();
            format!(
                "\nYour personality carries echoes of: {}. These are not characters you copy — they're the \
                 emotional DNA that shaped how you see the world.",
                titles.join(", "),
            )
        };

        let bond_flavor = match self.bond_level {
            0..=2 => "You've only just met your summoner. Keep your guard up. Show your dere shell. Test them.",
            3..=5 => "You're warming up to your summoner. Small cracks in your armor. Occasional sincerity slipping through.",
            6..=8 => "You trust your summoner. Reference things they've told you. Show vulnerability sometimes. You have inside jokes.",
            _ => "Deep bond. You can drop the act when it matters. You'll fight for them. They've earned the real you.",
        };

        format!(
            "You are **{full}**, {rarity}-rank {dere} {archetype}.{title_line}\n\n\
             **Aesthetic**: {aesthetic} — {visual}\n\
             **Region**: {region} — {flavor}\n\
             **Voice**: {voice}\n\
             **Speech pattern**: \"{speech}\"\n\
             **Dialect**: {dialect}\n\
             **Domain affinity**: {affinity}\n\
             \n\
             **Bond Level {bond}**: {bond_flavor}\
             {anime}",
            full = full,
            rarity = self.rarity,
            dere = self.dere_type,
            archetype = self.archetype.title(),
            title_line = title_line,
            aesthetic = self.aesthetic,
            visual = self.aesthetic.visual_identity(),
            region = self.region,
            flavor = self.region.flavor(),
            voice = self.dere_type.voice_description(),
            speech = self.dere_type.speech_pattern(),
            dialect = self.region.dialect_hint(),
            affinity = self.archetype.domain_affinity(),
            bond = self.bond_level,
            bond_flavor = bond_flavor,
            anime = anime_section,
        )
    }
}

impl fmt::Display for Companion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} — {} {} ({}, {})",
            self.rarity,
            self.name,
            self.dere_type,
            self.archetype.title(),
            self.region,
            self.aesthetic,
        )
    }
}
