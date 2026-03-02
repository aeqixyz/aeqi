use axum::extract::{Json, State};
use axum::http::StatusCode;
use std::sync::Arc;

use crate::AppState;
use crate::auth::AuthTenant;
use crate::types::*;

/// Create a Stripe Checkout Session for a tier upgrade.
pub async fn checkout(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckoutRequest>,
) -> Result<Json<CheckoutResponse>, (StatusCode, String)> {
    let stripe_config = state.platform.stripe.as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "stripe not configured".to_string()))?;

    let price_id = match req.tier.as_str() {
        "basic" => &stripe_config.price_basic,
        "pro" => &stripe_config.price_pro,
        _ => return Err((StatusCode::BAD_REQUEST, "invalid tier".to_string())),
    };

    let email = state.manager.get_tenant_email(&tenant.id.0).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .unwrap_or_default();

    let url = system_tenants::stripe::create_checkout_session(
        stripe_config,
        &tenant.id.0,
        &email,
        price_id,
        "https://app.gacha.agency/app?checkout=success",
        "https://app.gacha.agency/app/upgrade?checkout=cancelled",
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(CheckoutResponse { url }))
}

/// Handle Stripe webhook events.
pub async fn webhook(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<Json<SuccessResponse>, (StatusCode, String)> {
    // Parse the raw body as a Stripe event (skip signature verification for now;
    // signature header will be added when Stripe config is wired).
    let event: system_tenants::stripe::StripeEvent = serde_json::from_str(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid event: {e}")))?;

    match event.event_type.as_str() {
        "checkout.session.completed" => {
            let metadata = event.data.object.get("metadata").cloned().unwrap_or_default();
            let tenant_id = metadata.get("tenant_id").and_then(|v| v.as_str()).unwrap_or("");
            let customer_id = event.data.object.get("customer").and_then(|v| v.as_str()).unwrap_or("");
            let subscription_id = event.data.object.get("subscription").and_then(|v| v.as_str()).unwrap_or("");

            if !tenant_id.is_empty() && !customer_id.is_empty() {
                let _ = state.manager.set_stripe_customer(tenant_id, customer_id).await;
                if !subscription_id.is_empty() {
                    let _ = state.manager.set_stripe_subscription(tenant_id, subscription_id).await;
                }

                // Determine tier from price
                let stripe_config = state.platform.stripe.as_ref();
                if let Some(sc) = stripe_config {
                    let line_items = event.data.object.get("line_items")
                        .and_then(|li| li.get("data"))
                        .and_then(|d| d.as_array());

                    if let Some(items) = line_items {
                        for item in items {
                            if let Some(price_id) = item.get("price").and_then(|p| p.get("id")).and_then(|id| id.as_str()) {
                                let tier = if price_id == sc.price_basic {
                                    "basic"
                                } else if price_id == sc.price_pro {
                                    "pro"
                                } else {
                                    continue;
                                };
                                let _ = state.manager.update_tier(tenant_id, tier).await;
                            }
                        }
                    }
                }
            }
        }
        "customer.subscription.deleted" => {
            // Downgrade to free tier
            let customer_id = event.data.object.get("customer").and_then(|v| v.as_str()).unwrap_or("");
            if !customer_id.is_empty() {
                // Look up tenant by customer ID via manager method
                let tenant_id = {
                    let db = state.manager.db().await;
                    db.query_row(
                        "SELECT id FROM tenants WHERE stripe_customer_id = ?1",
                        rusqlite::params![customer_id],
                        |row| row.get::<_, String>(0),
                    ).ok()
                };
                if let Some(tid) = tenant_id {
                    let _ = state.manager.update_tier(&tid, "free").await;
                }
            }
        }
        _ => {} // Ignore other events
    }

    Ok(Json(SuccessResponse { success: true }))
}

/// Create a Stripe Billing Portal Session.
pub async fn portal(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
) -> Result<Json<PortalResponse>, (StatusCode, String)> {
    let stripe_config = state.platform.stripe.as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "stripe not configured".to_string()))?;

    let customer_id = state.manager.get_stripe_customer(&tenant.id.0).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::BAD_REQUEST, "no stripe subscription".to_string()))?;

    let url = system_tenants::stripe::create_portal_session(
        stripe_config,
        &customer_id,
        "https://app.gacha.agency/app/settings",
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(PortalResponse { url }))
}
