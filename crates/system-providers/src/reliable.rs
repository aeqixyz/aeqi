use anyhow::Result;
use async_trait::async_trait;
use rand::Rng;
use system_core::traits::{ChatRequest, ChatResponse, Provider};
use std::sync::Arc;
use tracing::{info, warn};

/// A provider that wraps multiple providers with fallback and retry.
pub struct ReliableProvider {
    providers: Vec<Arc<dyn Provider>>,
    max_retries: u32,
}

impl ReliableProvider {
    pub fn new(providers: Vec<Arc<dyn Provider>>) -> Self {
        Self {
            providers,
            max_retries: 2,
        }
    }

    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }
}

#[async_trait]
impl Provider for ReliableProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let mut last_error = None;

        for provider in &self.providers {
            for attempt in 0..=self.max_retries {
                match provider.chat(request).await {
                    Ok(response) => return Ok(response),
                    Err(e) => {
                        warn!(
                            provider = %provider.name(),
                            attempt = attempt + 1,
                            error = %e,
                            "provider request failed"
                        );
                        last_error = Some(e);

                        if attempt < self.max_retries {
                            let base_ms = 500 * 2u64.pow(attempt);
                            let jitter_ms = rand::rng().random_range(0..=base_ms / 2);
                            let delay = std::time::Duration::from_millis(base_ms + jitter_ms);
                            tokio::time::sleep(delay).await;
                        }
                    }
                }
            }
            info!(
                provider = %provider.name(),
                "exhausted retries, trying next provider"
            );
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("no providers configured")))
    }

    fn name(&self) -> &str {
        "reliable"
    }

    async fn health_check(&self) -> Result<()> {
        for provider in &self.providers {
            if provider.health_check().await.is_ok() {
                return Ok(());
            }
        }
        anyhow::bail!("all providers unhealthy")
    }
}
