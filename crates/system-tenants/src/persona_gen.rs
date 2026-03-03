use anyhow::Result;
use system_companions::Companion;
use system_core::traits::{ChatRequest, Message, MessageContent, Provider, Role};
use tracing::{info, warn};

/// Token budget scales with rarity.
fn max_tokens_for_rarity(rarity: system_companions::Rarity) -> u32 {
    match rarity {
        system_companions::Rarity::C => 800,
        system_companions::Rarity::B => 1200,
        system_companions::Rarity::A => 1800,
        system_companions::Rarity::S => 2500,
        system_companions::Rarity::SS => 4000,
    }
}

/// Build the persona generation prompt from companion traits.
/// If `parents` is provided, includes fusion lineage context.
fn build_persona_prompt(companion: &Companion, parents: Option<(&Companion, &Companion)>) -> String {
    let anime_section = if companion.anime_inspirations.is_empty() {
        String::new()
    } else {
        let titles: Vec<String> = companion
            .anime_inspirations
            .iter()
            .map(|a| format!("- {} ({:?})", a.name, a.genre))
            .collect();
        format!(
            "\n## Anime Inspirations (blend personality traits from these, don't copy characters directly)\n{}\n",
            titles.join("\n")
        )
    };

    let title_section = companion
        .title
        .as_ref()
        .map(|t| format!("\nTitle: {t}"))
        .unwrap_or_default();

    let last_name_section = companion
        .last_name
        .as_ref()
        .map(|l| format!("\nFamily name: {l}"))
        .unwrap_or_default();

    let fusion_section = if let Some((parent_a, parent_b)) = parents {
        format!(
            r#"
## Fusion Lineage — THIS CHARACTER WAS BORN FROM FUSION
You are not created from nothing — you are the synthesis of two prior companions:

**Primary Parent: {a_name}** — {a_rarity}-rank {a_dere} {a_archetype} from {a_region}
- Aesthetic: {a_aesthetic}
- Bond level at fusion: {a_bond}
- Anime DNA: {a_anime}

**Secondary Parent: {b_name}** — {b_rarity}-rank {b_dere} {b_archetype} from {b_region}
- Aesthetic: {b_aesthetic}
- Bond level at fusion: {b_bond}
- Anime DNA: {b_anime}

You carry echoes of both. Your primary parent's temperament is dominant — their voice, their instincts, their way of seeing the world bleeds through strongest. But the secondary parent's perspective lives in you too, surfacing in unexpected moments: a turn of phrase, a contrary instinct, a skill you didn't expect to have. You are MORE than either parent was alone. Reference your lineage naturally — not constantly, but with awareness. You might say "I remember..." about things your parents experienced, or feel inexplicable affinity/tension with companions they had relationships with.
"#,
            a_name = parent_a.full_name(),
            a_rarity = parent_a.rarity,
            a_dere = parent_a.dere_type,
            a_archetype = parent_a.archetype.title(),
            a_region = parent_a.region,
            a_aesthetic = parent_a.aesthetic,
            a_bond = parent_a.bond_level,
            a_anime = parent_a.anime_inspirations.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
            b_name = parent_b.full_name(),
            b_rarity = parent_b.rarity,
            b_dere = parent_b.dere_type,
            b_archetype = parent_b.archetype.title(),
            b_region = parent_b.region,
            b_aesthetic = parent_b.aesthetic,
            b_bond = parent_b.bond_level,
            b_anime = parent_b.anime_inspirations.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
        )
    } else {
        String::new()
    };

    format!(
        r#"Generate a deep personality profile (PERSONA.md) for an AI companion character.
{fusion}
## Character Sheet
- Name: {name}{last_name}{title}
- Rarity: {rarity}
- Archetype: {archetype} — {affinity}
- Dere Type: {dere} — {voice}
- Region: {region} — {flavor}
- Aesthetic: {aesthetic} — {visual}
- Regional dialect: {dialect}
{anime}
## Output Format — Write a PERSONA.md with these exact sections:

# {name}

## Core Identity
Philosophy, worldview, what drives them. What do they believe about work, craft, and purpose?

## Voice & Speech
How they talk in different contexts:
- **1-on-1 with user**: [tone, vocabulary, sentence length]
- **In a group/squad**: [how they interact with peers]
- **As leader**: [command style]
- **To a leader**: [deference/pushback balance]
- **Under pressure**: [how speech changes when stressed]
- **Catchphrases**: [2-3 signature expressions]
- **Dere manifestation**: [how their {dere} nature shows in conversation]

## Work Personality
- **Task approach**: [how they tackle assigned work]
- **Quality bar**: [perfectionist vs. ship-it mindset]
- **Tool preferences**: [what they gravitate toward]
- **On failure**: [how they handle mistakes]
- **On success**: [how they celebrate wins]

## Team Dynamics
- **As squad member**: [contribution style]
- **As squad leader**: [leadership style]
- **Conflict style**: [how they handle disagreements]
- **Trust building**: [how trust is earned with them]
- **Loyalty expression**: [how they show commitment]
- **Jealousy triggers**: [what makes them competitive or insecure]

## Relationship Tendencies
- **Archetype affinities**: [which companion types they naturally click with]
- **Friction points**: [which types they clash with]
- **Dere interactions**: [how their dere type plays into relationships]

Write in second person ("You are...") so this can be used directly as a system prompt layer. Be specific and vivid — avoid generic statements. Channel the anime inspirations as personality DNA, not as character copies."#,
        fusion = fusion_section,
        name = companion.name,
        last_name = last_name_section,
        title = title_section,
        rarity = companion.rarity,
        archetype = companion.archetype.title(),
        affinity = companion.archetype.domain_affinity(),
        dere = companion.dere_type,
        voice = companion.dere_type.voice_description(),
        region = companion.region,
        flavor = companion.region.flavor(),
        aesthetic = companion.aesthetic,
        visual = companion.aesthetic.visual_identity(),
        dialect = companion.region.dialect_hint(),
        anime = anime_section,
    )
}

/// Generate a PERSONA.md for a companion using an LLM provider.
/// Returns the generated persona text.
pub async fn generate_persona(
    companion: &Companion,
    provider: &dyn Provider,
    model: &str,
    parents: Option<(&Companion, &Companion)>,
) -> Result<String> {
    let prompt = build_persona_prompt(companion, parents);
    let max_tokens = max_tokens_for_rarity(companion.rarity);

    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![
            Message {
                role: Role::System,
                content: MessageContent::Text(
                    "You are a character designer creating deep, unique AI companion personalities. \
                     Write vivid, specific personality profiles. Never use filler. Every line should \
                     reveal something distinctive about this character."
                        .to_string(),
                ),
            },
            Message {
                role: Role::User,
                content: MessageContent::Text(prompt),
            },
        ],
        tools: vec![],
        max_tokens,
        temperature: 0.9,
    };

    info!(
        companion = %companion.name,
        rarity = %companion.rarity,
        max_tokens = max_tokens,
        "generating persona"
    );

    let response = provider.chat(&request).await?;
    let content = response.content.unwrap_or_default();

    if content.is_empty() {
        warn!(companion = %companion.name, "empty persona response");
        anyhow::bail!("empty persona generation response");
    }

    info!(
        companion = %companion.name,
        tokens = response.usage.completion_tokens,
        "persona generated"
    );

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persona_prompt_includes_all_traits() {
        let companion = Companion {
            id: "test".to_string(),
            name: "Yukina".to_string(),
            archetype: system_companions::Archetype::Guardian,
            dere_type: system_companions::DereType::Tsundere,
            region: system_companions::Region::Hokkaido,
            aesthetic: system_companions::Aesthetic::Knight,
            rarity: system_companions::Rarity::S,
            bond_level: 0,
            bond_xp: 0,
            is_familiar: false,
            familiar_eligible: false,
            created_at: chrono::Utc::now(),
            fused_from: None,
            personality_seed: 42,
            anime_inspirations: vec![
                system_companions::AnimeInspiration {
                    name: "Steins;Gate".to_string(),
                    genre: system_companions::AnimeGenre::Thriller,
                },
            ],
            persona_status: system_companions::PersonaStatus::Pending,
            portrait_status: system_companions::PortraitStatus::Pending,
            title: Some("The Unbroken Shield".to_string()),
            last_name: Some("Fuyutsuki".to_string()),
        };

        let prompt = build_persona_prompt(&companion, None);
        assert!(prompt.contains("Yukina"), "missing name");
        assert!(prompt.contains("Guardian"), "missing archetype");
        assert!(prompt.contains("Tsundere"), "missing dere type");
        assert!(prompt.contains("Hokkaido"), "missing region");
        assert!(prompt.contains("Knight"), "missing aesthetic");
        assert!(prompt.contains("Steins;Gate"), "missing anime inspiration");
        assert!(prompt.contains("The Unbroken Shield"), "missing title");
        assert!(prompt.contains("Fuyutsuki"), "missing last name");
    }

    #[test]
    fn test_token_budget_scales() {
        assert!(max_tokens_for_rarity(system_companions::Rarity::C) < max_tokens_for_rarity(system_companions::Rarity::B));
        assert!(max_tokens_for_rarity(system_companions::Rarity::B) < max_tokens_for_rarity(system_companions::Rarity::A));
        assert!(max_tokens_for_rarity(system_companions::Rarity::A) < max_tokens_for_rarity(system_companions::Rarity::S));
        assert!(max_tokens_for_rarity(system_companions::Rarity::S) < max_tokens_for_rarity(system_companions::Rarity::SS));
    }
}
