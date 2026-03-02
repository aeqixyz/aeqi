/// Model pricing table — maps model name patterns to USD per million tokens.
/// Prices are approximate and should be updated periodically. When a model
/// isn't found, falls back to a conservative default to avoid zero-cost reporting.
pub struct ModelPrice {
    pub prompt_per_mtok: f64,
    pub completion_per_mtok: f64,
}

impl ModelPrice {
    pub fn cost(&self, prompt_tokens: u32, completion_tokens: u32) -> f64 {
        (prompt_tokens as f64 / 1_000_000.0) * self.prompt_per_mtok
            + (completion_tokens as f64 / 1_000_000.0) * self.completion_per_mtok
    }
}

static PRICING: &[(&str, ModelPrice)] = &[
    // Claude (Anthropic direct + OpenRouter)
    ("claude-opus-4", ModelPrice { prompt_per_mtok: 15.0, completion_per_mtok: 75.0 }),
    ("claude-sonnet-4", ModelPrice { prompt_per_mtok: 3.0, completion_per_mtok: 15.0 }),
    ("claude-haiku-4", ModelPrice { prompt_per_mtok: 0.80, completion_per_mtok: 4.0 }),
    ("claude-3.5-sonnet", ModelPrice { prompt_per_mtok: 3.0, completion_per_mtok: 15.0 }),
    ("claude-3-haiku", ModelPrice { prompt_per_mtok: 0.25, completion_per_mtok: 1.25 }),
    // OpenRouter routed
    ("anthropic/claude-opus", ModelPrice { prompt_per_mtok: 15.0, completion_per_mtok: 75.0 }),
    ("anthropic/claude-sonnet", ModelPrice { prompt_per_mtok: 3.0, completion_per_mtok: 15.0 }),
    ("anthropic/claude-haiku", ModelPrice { prompt_per_mtok: 0.80, completion_per_mtok: 4.0 }),
    // MiniMax
    ("minimax/minimax-m2.5", ModelPrice { prompt_per_mtok: 0.50, completion_per_mtok: 1.50 }),
    ("minimax/minimax-m1", ModelPrice { prompt_per_mtok: 0.30, completion_per_mtok: 1.00 }),
    // DeepSeek
    ("deepseek/deepseek-v3", ModelPrice { prompt_per_mtok: 0.27, completion_per_mtok: 1.10 }),
    ("deepseek/deepseek-r1", ModelPrice { prompt_per_mtok: 0.55, completion_per_mtok: 2.19 }),
    ("deepseek/deepseek-chat", ModelPrice { prompt_per_mtok: 0.14, completion_per_mtok: 0.28 }),
    // GPT-4o
    ("openai/gpt-4o", ModelPrice { prompt_per_mtok: 2.50, completion_per_mtok: 10.0 }),
    ("openai/gpt-4o-mini", ModelPrice { prompt_per_mtok: 0.15, completion_per_mtok: 0.60 }),
    // Gemini
    ("google/gemini-2.5-pro", ModelPrice { prompt_per_mtok: 1.25, completion_per_mtok: 10.0 }),
    ("google/gemini-2.5-flash", ModelPrice { prompt_per_mtok: 0.15, completion_per_mtok: 0.60 }),
    // Llama (typically via OpenRouter)
    ("meta-llama/llama-4", ModelPrice { prompt_per_mtok: 0.20, completion_per_mtok: 0.80 }),
    ("meta-llama/llama-3", ModelPrice { prompt_per_mtok: 0.10, completion_per_mtok: 0.40 }),
    // Ollama (local — free)
    ("ollama/", ModelPrice { prompt_per_mtok: 0.0, completion_per_mtok: 0.0 }),
];

const DEFAULT_PRICE: ModelPrice = ModelPrice {
    prompt_per_mtok: 1.0,
    completion_per_mtok: 3.0,
};

pub fn lookup(model: &str) -> &ModelPrice {
    let model_lower = model.to_lowercase();
    for (prefix, price) in PRICING {
        if model_lower.starts_with(prefix) {
            return price;
        }
    }
    &DEFAULT_PRICE
}

pub fn estimate_cost(model: &str, prompt_tokens: u32, completion_tokens: u32) -> f64 {
    lookup(model).cost(prompt_tokens, completion_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_model_pricing() {
        let cost = estimate_cost("minimax/minimax-m2.5", 1_000_000, 1_000_000);
        assert!((cost - 2.0).abs() < 0.01); // $0.50 + $1.50
    }

    #[test]
    fn test_claude_opus_pricing() {
        let cost = estimate_cost("claude-opus-4-6", 10_000, 1_000);
        let expected = (10_000.0 / 1_000_000.0) * 15.0 + (1_000.0 / 1_000_000.0) * 75.0;
        assert!((cost - expected).abs() < 0.0001);
    }

    #[test]
    fn test_openrouter_routed_model() {
        let cost = estimate_cost("anthropic/claude-sonnet-4-6", 50_000, 5_000);
        let expected = (50_000.0 / 1_000_000.0) * 3.0 + (5_000.0 / 1_000_000.0) * 15.0;
        assert!((cost - expected).abs() < 0.0001);
    }

    #[test]
    fn test_unknown_model_uses_default() {
        let cost = estimate_cost("some-unknown-model/v99", 1_000_000, 1_000_000);
        assert!((cost - 4.0).abs() < 0.01); // $1.0 + $3.0 default
    }

    #[test]
    fn test_ollama_is_free() {
        let cost = estimate_cost("ollama/llama3:70b", 1_000_000, 1_000_000);
        assert!(cost.abs() < 0.001);
    }

    #[test]
    fn test_zero_tokens() {
        let cost = estimate_cost("claude-opus-4-6", 0, 0);
        assert!(cost.abs() < 0.0001);
    }
}
