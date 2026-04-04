use anyhow::Result;
use std::path::PathBuf;

use crate::cli::TriggerAction;
use crate::helpers::load_config;

pub(crate) async fn cmd_trigger(
    config_path: &Option<PathBuf>,
    action: TriggerAction,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let registry = aeqi_orchestrator::agent_registry::AgentRegistry::open(&config.data_dir())?;
    let trigger_store = registry.trigger_store();

    match action {
        TriggerAction::Create {
            name,
            agent,
            schedule,
            at,
            event,
            event_company,
            event_tool,
            cooldown,
            webhook,
            signing_secret,
            skill,
            max_budget,
        } => {
            // Resolve agent by name.
            let agents = registry.get_by_name(&agent).await?;
            let pa = agents
                .into_iter()
                .find(|a| a.status == aeqi_orchestrator::agent_registry::AgentStatus::Active)
                .ok_or_else(|| anyhow::anyhow!("no active agent named '{agent}'"))?;

            // Build trigger type.
            let trigger_type = if let Some(schedule) = schedule {
                aeqi_orchestrator::trigger::TriggerType::Schedule { expr: schedule }
            } else if let Some(at_str) = at {
                let at = chrono::DateTime::parse_from_rfc3339(&at_str)
                    .map_err(|e| anyhow::anyhow!("invalid timestamp: {e}"))?
                    .with_timezone(&chrono::Utc);
                aeqi_orchestrator::trigger::TriggerType::Once { at }
            } else if let Some(event_name) = event {
                let cooldown_secs = cooldown.unwrap_or(300);
                if cooldown_secs < 60 {
                    anyhow::bail!("cooldown must be >= 60 seconds");
                }
                let pattern = match event_name.as_str() {
                    "quest_completed" => aeqi_orchestrator::trigger::EventPattern::QuestCompleted {
                        project: event_company.clone(),
                    },
                    "quest_failed" => aeqi_orchestrator::trigger::EventPattern::QuestFailed {
                        project: event_company,
                    },
                    "tool_call_completed" => {
                        aeqi_orchestrator::trigger::EventPattern::ToolCallCompleted {
                            tool: event_tool,
                        }
                    }
                    other => anyhow::bail!("unknown event: {other}"),
                };
                aeqi_orchestrator::trigger::TriggerType::Event {
                    pattern,
                    cooldown_secs,
                }
            } else if webhook {
                let public_id = aeqi_orchestrator::trigger::generate_webhook_public_id();
                aeqi_orchestrator::trigger::TriggerType::Webhook {
                    public_id,
                    signing_secret,
                }
            } else {
                anyhow::bail!("provide --schedule, --at, --event, or --webhook");
            };

            let trigger = trigger_store
                .create(&aeqi_orchestrator::trigger::NewTrigger {
                    agent_id: pa.id.clone(),
                    name: name.clone(),
                    trigger_type,
                    skill: skill.clone(),
                    max_budget_usd: max_budget,
                })
                .await?;

            println!("Trigger created:");
            println!("  ID:     {}", trigger.id);
            println!("  Name:   {}", trigger.name);
            println!("  Agent:  {} ({})", pa.name, pa.id);
            println!("  Type:   {}", trigger.trigger_type.type_str());
            println!("  Skill:  {}", trigger.skill);
            if let aeqi_orchestrator::trigger::TriggerType::Webhook {
                public_id,
                signing_secret,
            } = &trigger.trigger_type
            {
                println!("  URL:    POST /api/webhooks/{public_id}");
                if signing_secret.is_some() {
                    println!("  Auth:   HMAC-SHA256 (X-Signature-256 header)");
                }
            }
            if let Some(b) = trigger.max_budget_usd {
                println!("  Budget: ${b:.2}/fire");
            }
        }

        TriggerAction::List { agent } => {
            let triggers = if let Some(agent_name) = agent {
                let agents = registry.get_by_name(&agent_name).await?;
                let mut all = Vec::new();
                for a in &agents {
                    all.extend(trigger_store.list_for_agent(&a.id).await?);
                }
                all
            } else {
                trigger_store.list_all().await?
            };

            if triggers.is_empty() {
                println!("No triggers.");
                return Ok(());
            }

            println!(
                "{:<36} {:<15} {:<10} {:<20} {:<8} {:<6}",
                "ID", "NAME", "TYPE", "SKILL", "ENABLED", "FIRES"
            );
            println!("{}", "-".repeat(95));
            for t in &triggers {
                println!(
                    "{:<36} {:<15} {:<10} {:<20} {:<8} {:<6}",
                    t.id,
                    t.name,
                    t.trigger_type.type_str(),
                    t.skill,
                    if t.enabled { "yes" } else { "no" },
                    t.fire_count,
                );
            }
        }

        TriggerAction::Show { id } => {
            let trigger = trigger_store
                .get(&id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("trigger '{id}' not found"))?;

            println!("Trigger: {} ({})", trigger.name, trigger.id);
            println!("  Agent:      {}", trigger.agent_id);
            println!("  Type:       {}", trigger.trigger_type.type_str());
            println!("  Skill:      {}", trigger.skill);
            println!("  Enabled:    {}", trigger.enabled);
            println!("  Fires:      {}", trigger.fire_count);
            println!("  Cost:       ${:.4}", trigger.total_cost_usd);
            println!("  Created:    {}", trigger.created_at);
            if let aeqi_orchestrator::trigger::TriggerType::Webhook {
                public_id,
                signing_secret,
            } = &trigger.trigger_type
            {
                println!("  URL:        POST /api/webhooks/{public_id}");
                if signing_secret.is_some() {
                    println!("  Auth:       HMAC-SHA256 (X-Signature-256 header)");
                }
            }
            if let Some(lf) = trigger.last_fired {
                println!("  Last fired: {lf}");
            }
            if let Some(b) = trigger.max_budget_usd {
                println!("  Budget:     ${b:.2}/fire");
            }
        }

        TriggerAction::Enable { id } => {
            trigger_store.update_enabled(&id, true).await?;
            println!("Trigger '{id}' enabled.");
        }

        TriggerAction::Disable { id } => {
            trigger_store.update_enabled(&id, false).await?;
            println!("Trigger '{id}' disabled.");
        }

        TriggerAction::Delete { id } => {
            trigger_store.delete(&id).await?;
            println!("Trigger '{id}' deleted.");
        }
    }

    Ok(())
}
