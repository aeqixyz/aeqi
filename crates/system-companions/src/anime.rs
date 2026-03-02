use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::companion::Rarity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimeGenre {
    Psychological,
    Shonen,
    SliceOfLife,
    Existential,
    Thriller,
    RomanceDrama,
    DarkFantasy,
    Comedy,
}

impl AnimeGenre {
    pub const ALL: [Self; 8] = [
        Self::Psychological,
        Self::Shonen,
        Self::SliceOfLife,
        Self::Existential,
        Self::Thriller,
        Self::RomanceDrama,
        Self::DarkFantasy,
        Self::Comedy,
    ];
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimeInspiration {
    pub name: String,
    pub genre: AnimeGenre,
}

/// Pool of anime titles organized by genre, with standard (C-A) and deep (S-SS) tiers.
pub struct AnimePool;

impl AnimePool {
    fn standard(genre: AnimeGenre) -> &'static [&'static str] {
        match genre {
            AnimeGenre::Psychological => &[
                "Death Note", "Psycho-Pass", "Monster", "Paranoia Agent",
                "The Promised Neverland", "Classroom of the Elite", "Kakegurui",
                "Terror in Resonance", "Mirai Nikki", "Danganronpa", "Akagi",
                "Kaiji",
            ],
            AnimeGenre::Shonen => &[
                "Naruto", "One Piece", "Bleach", "Dragon Ball Z",
                "My Hero Academia", "Demon Slayer", "Jujutsu Kaisen",
                "Black Clover", "Fairy Tail", "Soul Eater",
                "Fire Force", "Blue Exorcist",
            ],
            AnimeGenre::SliceOfLife => &[
                "K-On!", "Clannad", "Barakamon", "March Comes in Like a Lion",
                "Laid-Back Camp", "Non Non Biyori", "A Place Further Than the Universe",
                "Silver Spoon", "Hyouka", "Tanaka-kun is Always Listless",
                "Usagi Drop", "Sweetness & Lightning",
            ],
            AnimeGenre::Existential => &[
                "Neon Genesis Evangelion", "Ghost in the Shell: SAC",
                "Ergo Proxy", "Texhnolyze", "Haibane Renmei",
                "Casshern Sins", "Angel's Egg", "Kino's Journey",
                "Mushishi", "Mononoke", "The Tatami Galaxy",
                "Sonny Boy",
            ],
            AnimeGenre::Thriller => &[
                "Steins;Gate", "Code Geass", "Re:Zero", "Attack on Titan",
                "Erased", "Higurashi", "Another", "Shiki",
                "Future Diary", "From the New World", "91 Days",
                "Odd Taxi",
            ],
            AnimeGenre::RomanceDrama => &[
                "Your Lie in April", "Toradora!", "Clannad: After Story",
                "Fruits Basket", "Kaguya-sama", "Horimiya", "Oregairu",
                "Nana", "Lovely Complex", "Wotakoi", "Bunny Girl Senpai",
                "ReLife",
            ],
            AnimeGenre::DarkFantasy => &[
                "Berserk", "Claymore", "Goblin Slayer", "Vinland Saga",
                "Dororo", "Devilman Crybaby", "Made in Abyss",
                "The Rising of the Shield Hero", "Overlord", "Chainsaw Man",
                "Dorohedoro", "Hell's Paradise",
            ],
            AnimeGenre::Comedy => &[
                "Gintama", "Konosuba", "Nichijou", "Saiki K.",
                "Spy x Family", "Daily Lives of High School Boys",
                "Grand Blue", "Asobi Asobase", "Hinamatsuri",
                "Kaguya-sama (comedy arcs)", "Zombieland Saga",
                "The Devil is a Part-Timer!",
            ],
        }
    }

    fn deep(genre: AnimeGenre) -> &'static [&'static str] {
        match genre {
            AnimeGenre::Psychological => &[
                "Serial Experiments Lain", "Perfect Blue", "Paprika",
                "Magnetic Rose", "Boogiepop Phantom", "Paranoia Agent (deep cuts)",
            ],
            AnimeGenre::Shonen => &[
                "Hunter x Hunter (Chimera Ant)", "Fullmetal Alchemist: Brotherhood",
                "Mob Psycho 100", "Gintama (serious arcs)",
                "Rurouni Kenshin: Trust & Betrayal", "Yu Yu Hakusho (Chapter Black)",
            ],
            AnimeGenre::SliceOfLife => &[
                "Aria the Animation", "Yokohama Shopping Log",
                "Only Yesterday", "Whisper of the Heart",
                "Tamayura", "Sketchbook: Full Color's",
            ],
            AnimeGenre::Existential => &[
                "Legend of the Galactic Heroes", "Planetes",
                "Welcome to the NHK", "Memories (1995)",
                "Kaiba", "Land of the Lustrous",
            ],
            AnimeGenre::Thriller => &[
                "Monster (deep)", "Phantom: Requiem for the Phantom",
                "Rainbow", "Banana Fish",
                "ID: Invaded", "Moriarty the Patriot",
            ],
            AnimeGenre::RomanceDrama => &[
                "Ef: A Tale of Memories", "White Album 2",
                "5 Centimeters Per Second", "The Wind Rises",
                "Scum's Wish", "Given",
            ],
            AnimeGenre::DarkFantasy => &[
                "Puella Magi Madoka Magica", "Fate/Zero",
                "Berserk (Golden Age)", "Vampire Hunter D: Bloodlust",
                "Shigurui", "Blade of the Immortal",
            ],
            AnimeGenre::Comedy => &[
                "Cromartie High School", "Excel Saga",
                "Panty & Stocking", "Pop Team Epic",
                "Space Dandy", "FLCL",
            ],
        }
    }
}

/// Pick 3 anime inspirations from different genre buckets.
/// SS rarity draws exclusively from deep pool; S draws 1 deep + 2 standard;
/// lower rarities draw from standard pool only.
pub fn pick_inspirations(rng: &mut impl Rng, rarity: Rarity) -> Vec<AnimeInspiration> {
    let mut genres: Vec<AnimeGenre> = AnimeGenre::ALL.to_vec();

    // Shuffle genres to pick 3 different ones.
    for i in (1..genres.len()).rev() {
        let j = rng.random_range(0..=i);
        genres.swap(i, j);
    }

    let selected_genres = &genres[..3];
    let mut inspirations = Vec::with_capacity(3);

    for (i, &genre) in selected_genres.iter().enumerate() {
        let pool = match rarity {
            Rarity::SS => AnimePool::deep(genre),
            Rarity::S if i == 0 => AnimePool::deep(genre),
            _ => AnimePool::standard(genre),
        };

        let title = pool[rng.random_range(0..pool.len())];
        inspirations.push(AnimeInspiration {
            name: title.to_string(),
            genre,
        });
    }

    inspirations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_genres_have_standard_titles() {
        for genre in AnimeGenre::ALL {
            let pool = AnimePool::standard(genre);
            assert!(pool.len() >= 10, "{genre:?} standard pool too small: {}", pool.len());
        }
    }

    #[test]
    fn test_all_genres_have_deep_titles() {
        for genre in AnimeGenre::ALL {
            let pool = AnimePool::deep(genre);
            assert!(pool.len() >= 5, "{genre:?} deep pool too small: {}", pool.len());
        }
    }

    #[test]
    fn test_picks_3_different_genres() {
        let mut rng = rand::rng();
        for _ in 0..100 {
            let picks = pick_inspirations(&mut rng, Rarity::A);
            assert_eq!(picks.len(), 3);
            // All genres different.
            assert_ne!(picks[0].genre, picks[1].genre);
            assert_ne!(picks[0].genre, picks[2].genre);
            assert_ne!(picks[1].genre, picks[2].genre);
        }
    }

    #[test]
    fn test_ss_draws_from_deep_pool() {
        let mut rng = rand::rng();
        // Run many times — all picks should be from deep pool.
        for _ in 0..50 {
            let picks = pick_inspirations(&mut rng, Rarity::SS);
            for pick in &picks {
                let deep = AnimePool::deep(pick.genre);
                assert!(
                    deep.contains(&pick.name.as_str()),
                    "SS pick '{}' not in deep pool for {:?}",
                    pick.name,
                    pick.genre,
                );
            }
        }
    }

    #[test]
    fn test_s_draws_first_from_deep() {
        let mut rng = rand::rng();
        for _ in 0..50 {
            let picks = pick_inspirations(&mut rng, Rarity::S);
            let deep = AnimePool::deep(picks[0].genre);
            assert!(
                deep.contains(&picks[0].name.as_str()),
                "S first pick '{}' not in deep pool for {:?}",
                picks[0].name,
                picks[0].genre,
            );
        }
    }

    #[test]
    fn test_c_rarity_draws_standard_only() {
        let mut rng = rand::rng();
        for _ in 0..50 {
            let picks = pick_inspirations(&mut rng, Rarity::C);
            for pick in &picks {
                let standard = AnimePool::standard(pick.genre);
                assert!(
                    standard.contains(&pick.name.as_str()),
                    "C pick '{}' not in standard pool for {:?}",
                    pick.name,
                    pick.genre,
                );
            }
        }
    }
}
