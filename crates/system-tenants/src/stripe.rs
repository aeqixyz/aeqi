use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::StripeConfig;

/// Create a Stripe Checkout Session for a tier upgrade.
pub async fn create_checkout_session(
    config: &StripeConfig,
    tenant_id: &str,
    email: &str,
    price_id: &str,
    success_url: &str,
    cancel_url: &str,
) -> Result<String> {
    let client = reqwest::Client::new();

    let params = vec![
        ("mode", "subscription".to_string()),
        ("customer_email", email.to_string()),
        ("success_url", success_url.to_string()),
        ("cancel_url", cancel_url.to_string()),
        ("line_items[0][price]", price_id.to_string()),
        ("line_items[0][quantity]", "1".to_string()),
        ("metadata[tenant_id]", tenant_id.to_string()),
    ];

    let res = client
        .post("https://api.stripe.com/v1/checkout/sessions")
        .basic_auth(&config.secret_key, None::<&str>)
        .form(&params)
        .send()
        .await
        .context("stripe checkout request failed")?;

    let body: serde_json::Value = res.json().await.context("stripe checkout parse failed")?;

    if let Some(url) = body.get("url").and_then(|v| v.as_str()) {
        Ok(url.to_string())
    } else {
        let err = body.get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        anyhow::bail!("stripe checkout failed: {}", err)
    }
}

/// Create a Stripe Billing Portal Session.
pub async fn create_portal_session(
    config: &StripeConfig,
    customer_id: &str,
    return_url: &str,
) -> Result<String> {
    let client = reqwest::Client::new();

    let params = vec![
        ("customer", customer_id),
        ("return_url", return_url),
    ];

    let res = client
        .post("https://api.stripe.com/v1/billing_portal/sessions")
        .basic_auth(&config.secret_key, None::<&str>)
        .form(&params)
        .send()
        .await
        .context("stripe portal request failed")?;

    let body: serde_json::Value = res.json().await.context("stripe portal parse failed")?;

    if let Some(url) = body.get("url").and_then(|v| v.as_str()) {
        Ok(url.to_string())
    } else {
        anyhow::bail!("stripe portal failed")
    }
}

/// Stripe webhook event (simplified).
#[derive(Debug, Deserialize)]
pub struct StripeEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: StripeEventData,
}

#[derive(Debug, Deserialize)]
pub struct StripeEventData {
    pub object: serde_json::Value,
}

/// Verify a Stripe webhook signature.
pub fn verify_webhook(payload: &[u8], signature: &str, secret: &str) -> Result<StripeEvent> {
    use sha2::Sha256;

    // Parse the signature header
    let mut timestamp = "";
    let mut sig_v1 = "";
    for part in signature.split(',') {
        let kv: Vec<&str> = part.split('=').collect();
        if kv.len() == 2 {
            match kv[0] {
                "t" => timestamp = kv[1],
                "v1" => sig_v1 = kv[1],
                _ => {}
            }
        }
    }

    if timestamp.is_empty() || sig_v1.is_empty() {
        anyhow::bail!("invalid stripe signature header");
    }

    // Compute expected signature
    let signed_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));
    use sha2::digest::Mac;
    let mut mac = hmac::Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .context("invalid webhook secret")?;
    mac.update(signed_payload.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());

    if expected != sig_v1 {
        anyhow::bail!("webhook signature verification failed");
    }

    // Parse the event
    let event: StripeEvent = serde_json::from_slice(payload)
        .context("failed to parse stripe event")?;

    Ok(event)
}
