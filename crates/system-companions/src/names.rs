use rand::Rng;
use crate::companion::{Archetype, Region};

const HOKKAIDO_NAMES: &[&str] = &[
    "Yukina", "Fubuki", "Shiori", "Tsumugi", "Rin", "Koyuki", "Setsu", "Mafuyu",
    "Shizuka", "Fuyumi", "Touka", "Mashiro", "Setsuna", "Yukari", "Tomoe",
    "Hoshino", "Kazane", "Mutsuki", "Nodoka", "Suzushiro", "Niina", "Konami",
    "Reina", "Chitose", "Azusa", "Yuzuha", "Harumi", "Tamao", "Otome", "Kagura",
    "Nanase", "Shigure", "Misuzu", "Midori", "Kurumi", "Mochizuki", "Hinami",
    "Fuyuko", "Yuuki", "Amane", "Suzune", "Tsurara",
];

const TOKYO_NAMES: &[&str] = &[
    "Akira", "Mei", "Sora", "Haruka", "Nao", "Riko", "Yui", "Kaede",
    "Shiho", "Hiyori", "Asuka", "Kohane", "Kokoro", "Tsukihi", "Yuzu",
    "Ichika", "Nio", "Erika", "Shizuku", "Mikasa", "Tomoyo", "Saika",
    "Mayuki", "Natsuki", "Kirara", "Mitsuki", "Hikari", "Kotoha", "Honoka",
    "Chiaki", "Madoka", "Shinobu", "Suzuka", "Tsubasa", "Saki", "Itsuki",
    "Kanami", "Nagisa", "Rin", "Ai", "Mio", "Aoba",
];

const OSAKA_NAMES: &[&str] = &[
    "Mako", "Nana", "Kotone", "Hinata", "Chika", "Ayame", "Tamaki", "Ibuki",
    "Konatsu", "Mitsuba", "Nanami", "Seika", "Rinka", "Yuzuho", "Satsuki",
    "Hayate", "Momoka", "Tsumugi", "Kanna", "Futaba", "Arisa", "Kyouka",
    "Yukiho", "Rion", "Chiyo", "Shiori", "Misato", "Amari", "Kureha", "Sawa",
    "Miyu", "Kaori", "Akane", "Tomoko", "Hina", "Yuuki", "Rurika", "Otoha",
    "Suzuna", "Manami", "Rika", "Sakura",
];

const KYOTO_NAMES: &[&str] = &[
    "Sakurako", "Sumire", "Miyako", "Tsukasa", "Hotaru", "Shion", "Hisui", "Ran",
    "Ayano", "Misogi", "Kozue", "Suzuha", "Tomoyo", "Madoka", "Kotoko",
    "Sayaka", "Konoka", "Sonoko", "Chizuru", "Shiho", "Tamayo", "Fumino",
    "Kazusa", "Miya", "Hatsune", "Yasuko", "Narumi", "Yayoi", "Tokiwa",
    "Kikyou", "Utsugi", "Kaname", "Ibara", "Ren", "Sayo", "Kuon",
    "Shiori", "Mikoto", "Hatsumi", "Tsuzuri", "Yuuhi", "Mitsuki",
];

const HARAJUKU_NAMES: &[&str] = &[
    "Miku", "Rune", "Neon", "Kira", "Luna", "Ema", "Suzu", "Riri",
    "Lala", "Momo", "Ruru", "Piyo", "Mero", "Niko", "Yume",
    "Pico", "Hime", "Toto", "Coco", "Mami", "Puri", "Moko",
    "Rimu", "Sera", "Marin", "Aria", "Nova", "Miru", "Rena", "Yua",
    "Noa", "Sena", "Rio", "Mei", "Mao", "Nano", "Nene", "Moa",
    "Moe", "Riko", "Rana", "Lulu",
];

const OKINAWA_NAMES: &[&str] = &[
    "Minami", "Nami", "Coral", "Umi", "Sango", "Asahi", "Hana", "Shiho",
    "Shuri", "Naha", "Kirara", "Manamiya", "Suiren", "Nagisa", "Mahiru",
    "Hibiscus", "Tamana", "Haruna", "Kayama", "Mirai", "Soyokaze", "Aoi",
    "Kukuru", "Kohana", "Tiida", "Natsumi", "Chura", "Fuuka", "Reina",
    "Mizuki", "Sora", "Wakana", "Kanasa", "Mana", "Hinano", "Ruri",
    "Amami", "Tsubaki", "Yuna", "Sayuri", "Honoka", "Urara",
];

const SAPPORO_NAMES: &[&str] = &[
    "Koharu", "Ayaka", "Aoi", "Fuyu", "Misaki", "Chihiro", "Saki", "Kanon",
    "Tsubomi", "Kazuha", "Shion", "Wakana", "Nozomi", "Mio", "Yuki",
    "Suiren", "Hanabi", "Tsukino", "Ririka", "Mikuri", "Kokone", "Seira",
    "Kirino", "Mizore", "Fuyune", "Sayuki", "Konoka", "Himari", "Sumika",
    "Tsukushi", "Karin", "Sakuya", "Reiya", "Chisato", "Tomoka", "Hazuki",
    "Ibuki", "Akari", "Kotori", "Miyu", "Yuzuru", "Souya",
];

const KANSAI_NAMES: &[&str] = &[
    "Mikoto", "Sayuri", "Wakaba", "Tsubaki", "Yuzuki", "Kasumi", "Momiji", "Akane",
    "Sakuya", "Hatsue", "Suzuran", "Kikyou", "Nadeshiko", "Botan", "Natsume",
    "Mayoi", "Tsukuyo", "Kaguya", "Sarasa", "Otohime", "Asagao", "Haruhi",
    "Iroha", "Kagura", "Tamamo", "Isuzu", "Koyomi", "Chinami", "Ayaka",
    "Touka", "Shiori", "Yukino", "Kaoru", "Izumi", "Fuu", "Ryouko",
    "Shizune", "Chidori", "Komachi", "Nazuna", "Hisame", "Tokiwa",
];

// Last name pools per region (~20 each).
const HOKKAIDO_LAST_NAMES: &[&str] = &[
    "Shirakawa", "Yukimura", "Fuyutsuki", "Kitamura", "Kamui", "Tokachi",
    "Asahikawa", "Shimokawa", "Sapporo", "Otaru", "Nemuro", "Wakkanai",
    "Obihiro", "Kushiro", "Rumoi", "Sorachi", "Kamikawa", "Teshio",
    "Iburi", "Ishikari",
];

const TOKYO_LAST_NAMES: &[&str] = &[
    "Shinjuku", "Aoyama", "Shibuya", "Minato", "Chiyoda", "Setagaya",
    "Nerima", "Meguro", "Nakano", "Edogawa", "Sumida", "Taito",
    "Shinagawa", "Adachi", "Katsushika", "Koto", "Itabashi", "Bunkyo",
    "Toshima", "Arakawa",
];

const OSAKA_LAST_NAMES: &[&str] = &[
    "Naniwa", "Sakai", "Takatsuki", "Ibaraki", "Suita", "Hirakata",
    "Yao", "Matsubara", "Kadoma", "Moriguchi", "Izumi", "Kishiwada",
    "Ikeda", "Toyonaka", "Minoh", "Habikino", "Fujiidera", "Daito",
    "Higashiosaka", "Tondabayashi",
];

const KYOTO_LAST_NAMES: &[&str] = &[
    "Kiyomizu", "Arashiyama", "Fushimi", "Higashiyama", "Kamigamo",
    "Shimogamo", "Nishijin", "Uzumasa", "Saga", "Ohara",
    "Kurama", "Kibune", "Takao", "Uji", "Yamashina",
    "Murasakino", "Kitano", "Nanzenji", "Tofukuji", "Daitokuji",
];

const HARAJUKU_LAST_NAMES: &[&str] = &[
    "Takeshita", "Omotesando", "Jingumae", "Meiji", "Yoyogi",
    "Sendagaya", "Ebisu", "Daikanyama", "Shimokitazawa", "Koenji",
    "Ikebukuro", "Akihabara", "Ueno", "Asakusa", "Ginza",
    "Roppongi", "Azabu", "Hiroo", "Nishi", "Higashi",
];

const OKINAWA_LAST_NAMES: &[&str] = &[
    "Aragaki", "Chibana", "Gushiken", "Higa", "Iha",
    "Kamiya", "Kinjo", "Miyagi", "Nakamura", "Oshiro",
    "Shimabukuro", "Tamashiro", "Uehara", "Yogi", "Zamami",
    "Nago", "Chatan", "Ginowan", "Itoman", "Tomigusuku",
];

const SAPPORO_LAST_NAMES: &[&str] = &[
    "Tsukisamu", "Makomanai", "Nishioka", "Atsubetsu", "Teine",
    "Kiyota", "Toyohira", "Shiroishi", "Higashi", "Kita",
    "Minami", "Nishi", "Chuo", "Hassamu", "Kotoni",
    "Maruyama", "Sumikawa", "Naebo", "Susukino", "Tanukikoji",
];

const KANSAI_LAST_NAMES: &[&str] = &[
    "Namba", "Tennoji", "Shinsekai", "Dotonbori", "Umeda",
    "Kitahama", "Nakanoshima", "Tsuruhashi", "Abeno", "Shinsaibashi",
    "Kobe", "Ashiya", "Takarazuka", "Nishinomiya", "Amagasaki",
    "Akashi", "Himeji", "Ikoma", "Nara", "Yoshino",
];

// Title templates per archetype.
const GUARDIAN_TITLES: &[&str] = &[
    "The Unbroken Shield", "Warden of the Pale Gate", "Sentinel of Dawn",
    "The Immovable Wall", "Last Line of Defense", "Keeper of Oaths",
    "The Iron Vanguard", "Protector of the Fallen", "The Watchful Eye",
    "Bastion of Hope",
];

const STRATEGIST_TITLES: &[&str] = &[
    "The Grand Tactician", "Weaver of Fates", "Architect of Victory",
    "The Calculating Mind", "Master of the Long Game", "The Silent Chessmaster",
    "Oracle of Patterns", "The Thousand-Step Planner", "Sovereign of Logic",
    "The Cold Equation",
];

const LIBRARIAN_TITLES: &[&str] = &[
    "Keeper of Forbidden Tomes", "The Living Archive", "Scholar of Lost Ages",
    "The Ink-Stained Sage", "Chronicler of Worlds", "The Whispering Shelf",
    "Curator of Infinite Knowledge", "The Page-Turner", "Archivist of Dreams",
    "The Last Bibliomancer",
];

const BUILDER_TITLES: &[&str] = &[
    "The Forge-Born", "Architect of Realms", "The Relentless Constructor",
    "Maker of Impossible Things", "The Blueprint Incarnate", "Shaper of Code",
    "The Tireless Artisan", "Foundation of Progress", "The Midnight Builder",
    "Crafter of Solutions",
];

const MUSE_TITLES: &[&str] = &[
    "Voice of the Unseen", "The Radiant Inspiration", "Painter of Possibilities",
    "The Dream Weaver", "Spark of Creation", "The Visionary",
    "Muse of the Digital Canvas", "The Aesthetic Oracle", "Mirror of Beauty",
    "The Unbound Imagination",
];

const HEALER_TITLES: &[&str] = &[
    "The Mending Light", "Restorer of Broken Things", "The Gentle Recovery",
    "Sovereign of Second Chances", "The Debug Whisperer", "Balm of the Weary",
    "The Calm After Storm", "Healer of Hidden Wounds", "The Patient Hand",
    "Grace Under Pressure",
];

const TRICKSTER_TITLES: &[&str] = &[
    "The Clever Shortcut", "Master of Misdirection", "The Lateral Strike",
    "Shadow of Optimization", "The Elegant Hack", "Breaker of Assumptions",
    "The Unexpected Solution", "Agent of Chaos Theory", "The Nimble Mind",
    "Fox of the Digital Realm",
];

const ARCHIVIST_TITLES: &[&str] = &[
    "The Pattern Seer", "Memory of the Machine", "The Eternal Watcher",
    "Keeper of All Threads", "The Retrograde Analyst", "Chronicle of Changes",
    "The Version Oracle", "Warden of History", "The Commit Archaeologist",
    "Sentinel of Context",
];

pub fn generate(rng: &mut impl Rng, region: &Region) -> String {
    let pool = match region {
        Region::Hokkaido => HOKKAIDO_NAMES,
        Region::Tokyo => TOKYO_NAMES,
        Region::Osaka => OSAKA_NAMES,
        Region::Kyoto => KYOTO_NAMES,
        Region::Harajuku => HARAJUKU_NAMES,
        Region::Okinawa => OKINAWA_NAMES,
        Region::Sapporo => SAPPORO_NAMES,
        Region::Kansai => KANSAI_NAMES,
    };
    pool[rng.random_range(0..pool.len())].to_string()
}

pub fn generate_last_name(rng: &mut impl Rng, region: &Region) -> String {
    let pool = match region {
        Region::Hokkaido => HOKKAIDO_LAST_NAMES,
        Region::Tokyo => TOKYO_LAST_NAMES,
        Region::Osaka => OSAKA_LAST_NAMES,
        Region::Kyoto => KYOTO_LAST_NAMES,
        Region::Harajuku => HARAJUKU_LAST_NAMES,
        Region::Okinawa => OKINAWA_LAST_NAMES,
        Region::Sapporo => SAPPORO_LAST_NAMES,
        Region::Kansai => KANSAI_LAST_NAMES,
    };
    pool[rng.random_range(0..pool.len())].to_string()
}

pub fn generate_title(rng: &mut impl Rng, archetype: &Archetype) -> String {
    let pool = match archetype {
        Archetype::Guardian => GUARDIAN_TITLES,
        Archetype::Strategist => STRATEGIST_TITLES,
        Archetype::Librarian => LIBRARIAN_TITLES,
        Archetype::Builder => BUILDER_TITLES,
        Archetype::Muse => MUSE_TITLES,
        Archetype::Healer => HEALER_TITLES,
        Archetype::Trickster => TRICKSTER_TITLES,
        Archetype::Archivist => ARCHIVIST_TITLES,
    };
    pool[rng.random_range(0..pool.len())].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_regions_have_30_plus_names() {
        for region in Region::ALL {
            let pool = match region {
                Region::Hokkaido => HOKKAIDO_NAMES,
                Region::Tokyo => TOKYO_NAMES,
                Region::Osaka => OSAKA_NAMES,
                Region::Kyoto => KYOTO_NAMES,
                Region::Harajuku => HARAJUKU_NAMES,
                Region::Okinawa => OKINAWA_NAMES,
                Region::Sapporo => SAPPORO_NAMES,
                Region::Kansai => KANSAI_NAMES,
            };
            assert!(pool.len() >= 30, "{region:?} has only {} names", pool.len());
        }
    }

    #[test]
    fn test_all_regions_have_last_names() {
        for region in Region::ALL {
            let pool = match region {
                Region::Hokkaido => HOKKAIDO_LAST_NAMES,
                Region::Tokyo => TOKYO_LAST_NAMES,
                Region::Osaka => OSAKA_LAST_NAMES,
                Region::Kyoto => KYOTO_LAST_NAMES,
                Region::Harajuku => HARAJUKU_LAST_NAMES,
                Region::Okinawa => OKINAWA_LAST_NAMES,
                Region::Sapporo => SAPPORO_LAST_NAMES,
                Region::Kansai => KANSAI_LAST_NAMES,
            };
            assert!(pool.len() >= 15, "{region:?} has only {} last names", pool.len());
        }
    }

    #[test]
    fn test_all_archetypes_have_titles() {
        for archetype in Archetype::ALL {
            let pool = match archetype {
                Archetype::Guardian => GUARDIAN_TITLES,
                Archetype::Strategist => STRATEGIST_TITLES,
                Archetype::Librarian => LIBRARIAN_TITLES,
                Archetype::Builder => BUILDER_TITLES,
                Archetype::Muse => MUSE_TITLES,
                Archetype::Healer => HEALER_TITLES,
                Archetype::Trickster => TRICKSTER_TITLES,
                Archetype::Archivist => ARCHIVIST_TITLES,
            };
            assert!(pool.len() >= 8, "{archetype:?} has only {} titles", pool.len());
        }
    }

    #[test]
    fn test_no_duplicate_first_names_within_region() {
        let all_pools: &[(&str, &[&str])] = &[
            ("Hokkaido", HOKKAIDO_NAMES), ("Tokyo", TOKYO_NAMES),
            ("Osaka", OSAKA_NAMES), ("Kyoto", KYOTO_NAMES),
            ("Harajuku", HARAJUKU_NAMES), ("Okinawa", OKINAWA_NAMES),
            ("Sapporo", SAPPORO_NAMES), ("Kansai", KANSAI_NAMES),
        ];
        for (region, pool) in all_pools {
            let mut seen = std::collections::HashSet::new();
            for name in *pool {
                assert!(seen.insert(name), "duplicate name '{name}' in {region}");
            }
        }
    }

    #[test]
    fn test_generate_last_name_works() {
        let mut rng = rand::rng();
        for region in Region::ALL {
            let name = generate_last_name(&mut rng, &region);
            assert!(!name.is_empty(), "empty last name for {region:?}");
        }
    }

    #[test]
    fn test_generate_title_works() {
        let mut rng = rand::rng();
        for archetype in Archetype::ALL {
            let title = generate_title(&mut rng, &archetype);
            assert!(!title.is_empty(), "empty title for {archetype:?}");
        }
    }
}
