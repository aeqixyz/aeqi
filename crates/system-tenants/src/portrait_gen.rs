use anyhow::Result;
use system_companions::Companion;
use system_providers::OpenRouterProvider;
use tracing::info;

const DEFAULT_IMAGE_MODEL: &str = "openai/gpt-5-image";

/// Build an image generation prompt from companion traits.
pub fn build_portrait_prompt(companion: &Companion) -> String {
    let visual = companion.aesthetic.visual_identity();
    let archetype_title = companion.archetype.title();
    let affinity = companion.archetype.domain_affinity();
    let region_flavor = companion.region.flavor();

    // Expression/pose based on dere type.
    let dere_pose = match companion.dere_type {
        system_companions::DereType::Tsundere => "crossed arms, sharp side-eye glance, slight blush, defiant expression",
        system_companions::DereType::Kuudere => "composed and still, cool distant gaze, minimal expression, elegant poise",
        system_companions::DereType::Dandere => "shy downward gaze, hands clasped together, gentle half-smile, soft posture",
        system_companions::DereType::Yandere => "intense unwavering stare, possessive smile, leaning forward slightly",
        system_companions::DereType::Genki => "dynamic energetic pose, bright wide smile, peace sign or fist pump, sparkling eyes",
        system_companions::DereType::Deredere => "warm open smile, hands behind back, gentle loving eyes, relaxed posture",
        system_companions::DereType::Oneesama => "confident knowing smile, one hand on hip, mature elegant stance, half-lidded eyes",
    };

    // Detail level scales with rarity.
    let detail_level = match companion.rarity {
        system_companions::Rarity::C => "clean simple lineart, minimal shading, flat colors",
        system_companions::Rarity::B => "clean lineart with cel shading, some color depth",
        system_companions::Rarity::A => "detailed illustration, good lighting, rich colors",
        system_companions::Rarity::S => "highly detailed illustration, dynamic lighting, intricate details, rich palette",
        system_companions::Rarity::SS => "masterpiece quality, stunning detail, volumetric lighting, particle effects, aura, premium card art",
    };

    // Anime style references.
    let anime_style = if companion.anime_inspirations.is_empty() {
        String::new()
    } else {
        let titles: Vec<&str> = companion
            .anime_inspirations
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        format!("Art style influenced by: {}. ", titles.join(", "))
    };

    format!(
        "Anime character portrait, 3:4 aspect ratio, card game illustration, upper body focus.\n\
         \n\
         Character: {name}, {archetype_title} ({affinity}).\n\
         Visual identity: {visual}.\n\
         Expression and pose: {dere_pose}.\n\
         Setting/cultural hints: {region} — {region_flavor}.\n\
         {anime_style}\n\
         Quality: {detail_level}.\n\
         Style: high quality anime illustration, ecchi style, detailed expressive eyes, \
         beautiful character design, card game art, vibrant colors, clean composition.",
        name = companion.name,
        region = companion.region,
    )
}

/// Generate a portrait image for a companion using OpenRouter image generation.
/// Returns raw PNG/image bytes.
pub async fn generate_portrait(
    companion: &Companion,
    provider: &OpenRouterProvider,
    model: &str,
) -> Result<Vec<u8>> {
    let prompt = build_portrait_prompt(companion);
    let model = if model.is_empty() {
        DEFAULT_IMAGE_MODEL
    } else {
        model
    };

    info!(
        companion = %companion.name,
        rarity = %companion.rarity,
        model = model,
        "generating portrait"
    );

    let bytes = provider.generate_image(&prompt, model).await?;

    info!(
        companion = %companion.name,
        bytes = bytes.len(),
        "portrait generated"
    );

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_portrait_prompt_includes_traits() {
        let companion = Companion {
            id: "test".to_string(),
            name: "Sakura".to_string(),
            archetype: system_companions::Archetype::Muse,
            dere_type: system_companions::DereType::Genki,
            region: system_companions::Region::Harajuku,
            aesthetic: system_companions::Aesthetic::Sakura,
            rarity: system_companions::Rarity::A,
            bond_level: 0,
            bond_xp: 0,
            is_familiar: false,
            familiar_eligible: false,
            created_at: chrono::Utc::now(),
            fused_from: None,
            personality_seed: 42,
            anime_inspirations: vec![],
            persona_status: system_companions::PersonaStatus::Pending,
            portrait_status: system_companions::PortraitStatus::Pending,
            title: None,
            last_name: None,
        };

        let prompt = build_portrait_prompt(&companion);
        assert!(prompt.contains("Sakura"), "missing name");
        assert!(prompt.contains("Muse"), "missing archetype");
        assert!(prompt.contains("Harajuku"), "missing region");
        assert!(prompt.contains("energetic"), "missing genki pose");
        assert!(prompt.contains("ecchi"), "missing style");
        assert!(prompt.contains("3:4"), "missing aspect ratio");
    }

    #[test]
    fn test_rarity_affects_detail() {
        let make = |rarity| {
            let c = Companion {
                id: "test".to_string(),
                name: "Test".to_string(),
                archetype: system_companions::Archetype::Guardian,
                dere_type: system_companions::DereType::Tsundere,
                region: system_companions::Region::Tokyo,
                aesthetic: system_companions::Aesthetic::Knight,
                rarity,
                bond_level: 0,
                bond_xp: 0,
                is_familiar: false,
                familiar_eligible: false,
                created_at: chrono::Utc::now(),
                fused_from: None,
                personality_seed: 0,
                anime_inspirations: vec![],
                persona_status: system_companions::PersonaStatus::Pending,
                portrait_status: system_companions::PortraitStatus::Pending,
                title: None,
                last_name: None,
            };
            build_portrait_prompt(&c)
        };

        let c_prompt = make(system_companions::Rarity::C);
        let ss_prompt = make(system_companions::Rarity::SS);
        assert!(c_prompt.contains("simple lineart"));
        assert!(ss_prompt.contains("masterpiece"));
    }
}
