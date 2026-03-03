use anyhow::Result;
use std::path::Path;
use tracing::info;

use crate::tenant::TenantId;

/// Provision a new tenant's on-disk structure from templates.
pub fn provision_tenant(
    data_dir: &Path,
    template_dir: &Path,
    tenant_id: &TenantId,
    display_name: &str,
    tier: &str,
) -> Result<()> {
    // Create base directories.
    std::fs::create_dir_all(data_dir.join("agents/shared"))?;
    std::fs::create_dir_all(data_dir.join("projects/chat/.quests"))?;
    std::fs::create_dir_all(data_dir.join("projects/chat/.sigil"))?;

    // Write tenant metadata.
    let meta = crate::tenant::TenantMeta {
        id: tenant_id.0.clone(),
        display_name: display_name.to_string(),
        email: None,
        tier: tier.to_string(),
        created_at: chrono::Utc::now(),
        active_project: None,
        projects_source: None,
    };
    let meta_toml = toml::to_string_pretty(&meta)?;
    std::fs::write(data_dir.join("tenant.toml"), meta_toml)?;

    // Copy shared workflow from templates.
    let shared_workflow = template_dir.join("agents/shared/WORKFLOW.md");
    if shared_workflow.exists() {
        std::fs::copy(&shared_workflow, data_dir.join("agents/shared/WORKFLOW.md"))?;
    } else {
        // Minimal default.
        std::fs::write(
            data_dir.join("agents/shared/WORKFLOW.md"),
            "# Companion Workflow\n\nYou are a companion at gacha.agency. Be helpful, stay in character.\n",
        )?;
    }

    // Copy chat project template.
    let chat_template = template_dir.join("projects/chat/AGENTS.md");
    if chat_template.exists() {
        std::fs::copy(&chat_template, data_dir.join("projects/chat/AGENTS.md"))?;
    } else {
        std::fs::write(
            data_dir.join("projects/chat/AGENTS.md"),
            "# Chat Project\n\nYou are a conversational companion. Respond in character with personality.\n",
        )?;
    }

    // Write chat project KNOWLEDGE.md so agents know about available projects.
    let knowledge_template = template_dir.join("projects/chat/KNOWLEDGE.md");
    if knowledge_template.exists() {
        std::fs::copy(&knowledge_template, data_dir.join("projects/chat/KNOWLEDGE.md"))?;
    } else {
        std::fs::write(
            data_dir.join("projects/chat/KNOWLEDGE.md"),
            "# Chat Knowledge\n\n\
            ## Available Projects\n\
            - entity-legal: Legal entity formation and compliance\n\
            - algostaking: HFT trading system (Rust microservices)\n\
            - riftdecks-shop: TCG marketplace (Next.js)\n\
            - gacha-agency: Agent orchestration framework (Rust)\n\n\
            ## Your Role\n\
            You are a companion in the user's agency. Help them navigate their projects,\n\
            answer questions, and assist with tasks. If they mention a project, acknowledge\n\
            it exists and help them work on it.\n",
        )?;
    }

    info!(tenant = %tenant_id, dir = %data_dir.display(), "tenant provisioned");
    Ok(())
}

/// Materialize a companion as a full agent on disk (synchronous — fast, no LLM).
/// Writes fallback SOUL.md + IDENTITY.md immediately.
pub fn materialize_companion(
    data_dir: &Path,
    template_dir: &Path,
    companion: &system_companions::Companion,
) -> Result<std::path::PathBuf> {
    let agent_dir = data_dir.join("agents").join(&companion.name);
    std::fs::create_dir_all(agent_dir.join(".sigil"))?;

    // SOUL.md from companion personality + archetype template.
    let archetype_slug = format!("{:?}", companion.archetype).to_lowercase();
    let template_path = template_dir.join("agents/archetypes").join(format!("{archetype_slug}.md"));
    let archetype_template = if template_path.exists() {
        std::fs::read_to_string(&template_path).unwrap_or_default()
    } else {
        String::new()
    };

    let soul = format!(
        "# {}\n\n{}\n\n---\n\n{}",
        companion.name,
        companion.system_prompt_fragment(),
        archetype_template,
    );
    std::fs::write(agent_dir.join("SOUL.md"), &soul)?;

    // IDENTITY.md
    let full_name = companion.full_name();
    let identity = format!(
        "# Identity: {full_name}\n\nRarity: {} | Archetype: {} | Aesthetic: {} | Region: {}\nBond Level: {}\n",
        companion.rarity,
        companion.archetype.title(),
        companion.aesthetic,
        companion.region,
        companion.bond_level,
    );
    std::fs::write(agent_dir.join("IDENTITY.md"), &identity)?;

    // PREFERENCES.md (empty -- filled by interactions)
    std::fs::write(agent_dir.join("PREFERENCES.md"), "# Preferences\n\n*No preferences recorded yet.*\n")?;

    // MEMORY.md (empty)
    std::fs::write(agent_dir.join("MEMORY.md"), "# Memory\n\n*No memories recorded yet.*\n")?;

    // Emotional state (new)
    let emo = system_orchestrator::EmotionalState::new(&companion.name);
    emo.save(&system_orchestrator::EmotionalState::path_for_agent(&agent_dir))?;

    info!(companion = %companion.name, dir = %agent_dir.display(), "companion materialized (sync)");
    Ok(agent_dir)
}

/// Async portrait generation — calls image generation API to produce portrait.png.
/// Writes the image to the companion's agent directory and updates portrait_status.
pub async fn materialize_companion_portrait(
    data_dir: &Path,
    companion: &system_companions::Companion,
    platform: &crate::config::PlatformConfig,
    companion_store: &system_companions::CompanionStore,
) -> Result<()> {
    let agent_dir = data_dir.join("agents").join(&companion.name);
    std::fs::create_dir_all(&agent_dir)?;

    // Update status to generating.
    if let Ok(Some(mut c)) = companion_store.get_companion(&companion.id) {
        c.portrait_status = system_companions::PortraitStatus::Generating;
        let _ = companion_store.save_companion(&c);
    }

    // Build provider — use OpenRouter (same as persona gen).
    let provider = if let Some(ref openrouter) = platform.providers.openrouter {
        system_providers::OpenRouterProvider::new(
            openrouter.api_key.clone(),
            "openai/gpt-5-image".to_string(),
        )
    } else {
        // Update status to failed.
        if let Ok(Some(mut c)) = companion_store.get_companion(&companion.id) {
            c.portrait_status = system_companions::PortraitStatus::Failed;
            let _ = companion_store.save_companion(&c);
        }
        anyhow::bail!("no OpenRouter provider configured for portrait generation");
    };

    let model = "openai/gpt-5-image";

    match crate::portrait_gen::generate_portrait(companion, &provider, model).await {
        Ok(bytes) => {
            // Write portrait image.
            std::fs::write(agent_dir.join("portrait.png"), &bytes)?;

            // Update status to complete.
            if let Ok(Some(mut c)) = companion_store.get_companion(&companion.id) {
                c.portrait_status = system_companions::PortraitStatus::Complete;
                companion_store.save_companion(&c)?;
            }

            info!(companion = %companion.name, bytes = bytes.len(), "portrait written (async)");
            Ok(())
        }
        Err(e) => {
            // Update status to failed.
            if let Ok(Some(mut c)) = companion_store.get_companion(&companion.id) {
                c.portrait_status = system_companions::PortraitStatus::Failed;
                let _ = companion_store.save_companion(&c);
            }
            Err(e)
        }
    }
}

/// Async persona generation — calls LLM to generate PERSONA.md.
/// Identity::load() prefers PERSONA.md over SOUL.md, so this automatically
/// takes precedence once written.
pub async fn materialize_companion_persona(
    data_dir: &Path,
    companion: &system_companions::Companion,
    platform: &crate::config::PlatformConfig,
    parents: Option<(system_companions::Companion, system_companions::Companion)>,
) -> Result<()> {
    use system_core::traits::Provider;

    // Build provider — use OpenRouter with MiniMax M2.5 (cheap, high quality).
    let (provider, model): (Box<dyn Provider>, String) =
        if let Some(ref openrouter) = platform.providers.openrouter {
            (
                Box::new(system_providers::OpenRouterProvider::new(
                    openrouter.api_key.clone(),
                    "minimax/minimax-m2.5".to_string(),
                )),
                "minimax/minimax-m2.5".to_string(),
            )
        } else if let Some(ref anthropic) = platform.providers.anthropic {
            (
                Box::new(system_providers::AnthropicProvider::new(
                    anthropic.api_key.clone(),
                    "claude-haiku-4-5".to_string(),
                )),
                "claude-haiku-4-5".to_string(),
            )
        } else {
            anyhow::bail!("no provider configured for persona generation");
        };

    // Update persona status to generating.
    let agent_dir = data_dir.join("agents").join(&companion.name);

    let parent_refs = parents.as_ref().map(|(a, b)| (a, b));
    let persona_text = crate::persona_gen::generate_persona(companion, provider.as_ref(), &model, parent_refs).await?;

    // Write PERSONA.md — this takes precedence over SOUL.md.
    std::fs::create_dir_all(&agent_dir)?;
    std::fs::write(agent_dir.join("PERSONA.md"), &persona_text)?;

    info!(companion = %companion.name, "persona written (async)");
    Ok(())
}
