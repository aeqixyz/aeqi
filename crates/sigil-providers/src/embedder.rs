use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sigil_core::traits::Embedder;
use tracing::debug;

const OPENROUTER_EMBED_URL: &str = "https://openrouter.ai/api/v1/embeddings";

pub struct OpenRouterEmbedder {
    client: Client,
    api_key: String,
    model: String,
    dimensions: usize,
}

impl OpenRouterEmbedder {
    pub fn new(api_key: String, model: impl Into<String>, dimensions: usize) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            api_key,
            model: model.into(),
            dimensions,
        }
    }
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: &'a str,
}

#[derive(Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct EmbedError {
    error: EmbedErrorDetail,
}

#[derive(Deserialize)]
struct EmbedErrorDetail {
    message: String,
}

#[async_trait]
impl Embedder for OpenRouterEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        debug!(model = %self.model, len = text.len(), "embedding text");

        let resp = self
            .client
            .post(OPENROUTER_EMBED_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://sigil.dev")
            .header("X-Title", "System Memory")
            .json(&EmbedRequest {
                model: &self.model,
                input: text,
            })
            .send()
            .await
            .context("embedding request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Ok(e) = serde_json::from_str::<EmbedError>(&body) {
                anyhow::bail!("embedding API error ({status}): {}", e.error.message);
            }
            anyhow::bail!("embedding API error ({status}): {body}");
        }

        let parsed: EmbedResponse = resp
            .json()
            .await
            .context("failed to parse embedding response")?;
        parsed
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .context("no embedding data in response")
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}
