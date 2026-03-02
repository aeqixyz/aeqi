use axum::extract::{Json, State};
use axum::http::StatusCode;
use std::sync::Arc;

use crate::AppState;
use crate::auth::AuthTenant;
use crate::types::*;

use system_companions::GachaEngine;
use system_tenants::provision;
use system_tenants::auth::LoginResult;

// ── Anonymous registration (legacy) ──

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, (StatusCode, String)> {
    let display_name = req.display_name.unwrap_or_else(|| "Daemon".to_string());

    let (tenant, token) = state
        .manager
        .create_tenant(&display_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Pull a starter companion.
    let engine = GachaEngine::default();
    let mut pity = tenant.companion_store.load_pity()
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let starter = engine.pull(&mut pity);

    // Save to companion store.
    tenant
        .companion_store
        .save_companion(&starter)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Set as familiar.
    tenant
        .companion_store
        .set_familiar(&starter.id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Save pity state after pull.
    tenant.companion_store.save_pity(&pity)
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Materialize companion as agent on disk.
    let _ = provision::materialize_companion(
        &tenant.data_dir,
        &state.platform.template_dir(),
        &starter,
    );

    Ok(Json(RegisterResponse {
        token,
        tenant_id: tenant.id.to_string(),
        companion: CompanionInfo::from_companion(&starter),
    }))
}

// ── Email+password registration ──

pub async fn register_email(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EmailRegisterRequest>,
) -> Result<Json<EmailRegisterResponse>, (StatusCode, String)> {
    if req.password.len() < 8 {
        return Err((StatusCode::BAD_REQUEST, "password must be at least 8 characters".to_string()));
    }

    let (tenant, token, _verification_token) = state
        .manager
        .create_tenant_with_auth(&req.email, &req.password, req.display_name.as_deref())
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint") {
                (StatusCode::CONFLICT, "email already registered".to_string())
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
        })?;

    // Pull a starter companion.
    let engine = GachaEngine::default();
    let mut pity = tenant.companion_store.load_pity()
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let starter = engine.pull(&mut pity);

    tenant.companion_store.save_companion(&starter)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tenant.companion_store.set_familiar(&starter.id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tenant.companion_store.save_pity(&pity)
        .map_err(|e: anyhow::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let _ = provision::materialize_companion(
        &tenant.data_dir,
        &state.platform.template_dir(),
        &starter,
    );

    // Send verification email if configured.
    // For now, auto-verify since email service is optional.
    let has_email = state.platform.email.is_some();
    if !has_email {
        // Auto-verify if no email service configured.
        let _ = state.manager.verify_email(&_verification_token).await;
    }

    Ok(Json(EmailRegisterResponse {
        token,
        tenant_id: tenant.id.to_string(),
        companion: CompanionInfo::from_companion(&starter),
        requires_email_verification: has_email,
    }))
}

// ── Login ──

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    // If TOTP code provided with email+password, try direct login first then TOTP
    let result = state.manager.login(&req.email, &req.password).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match result {
        LoginResult::Success(token) => {
            Ok(Json(LoginResponse {
                token: Some(token),
                requires_totp: false,
                tenant_id: None,
            }))
        }
        LoginResult::RequiresTOTP(tenant_id) => {
            // If TOTP code was provided, verify it
            if let Some(code) = &req.totp_code {
                let token = state.manager.verify_totp_login(&tenant_id, code).await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                match token {
                    Some(t) => Ok(Json(LoginResponse {
                        token: Some(t),
                        requires_totp: false,
                        tenant_id: None,
                    })),
                    None => Err((StatusCode::UNAUTHORIZED, "invalid TOTP code".to_string())),
                }
            } else {
                Ok(Json(LoginResponse {
                    token: None,
                    requires_totp: true,
                    tenant_id: Some(tenant_id),
                }))
            }
        }
        LoginResult::InvalidCredentials => {
            Err((StatusCode::UNAUTHORIZED, "invalid email or password".to_string()))
        }
        LoginResult::EmailNotVerified => {
            Err((StatusCode::FORBIDDEN, "email not verified".to_string()))
        }
    }
}

// ── Email verification ──

pub async fn verify_email(
    State(state): State<Arc<AppState>>,
    Json(req): Json<VerifyEmailRequest>,
) -> Result<Json<VerifyEmailResponse>, (StatusCode, String)> {
    let verified = state.manager.verify_email(&req.token).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(VerifyEmailResponse { verified }))
}

// ── TOTP ──

pub async fn setup_totp(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
) -> Result<Json<TotpSetupResponse>, (StatusCode, String)> {
    let (secret, uri) = state.manager.setup_totp(&tenant.id.0).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(TotpSetupResponse { secret, uri }))
}

pub async fn verify_totp(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
    Json(req): Json<TotpVerifyRequest>,
) -> Result<Json<TotpVerifyResponse>, (StatusCode, String)> {
    let enabled = state.manager.enable_totp(&tenant.id.0, &req.code).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !enabled {
        return Err((StatusCode::BAD_REQUEST, "invalid TOTP code".to_string()));
    }
    Ok(Json(TotpVerifyResponse { enabled }))
}

pub async fn disable_totp(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
    Json(req): Json<TotpDisableRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, String)> {
    let success = state.manager.disable_totp(&tenant.id.0, &req.code).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !success {
        return Err((StatusCode::BAD_REQUEST, "invalid TOTP code".to_string()));
    }
    Ok(Json(SuccessResponse { success }))
}

// ── Password ──

pub async fn request_password_reset(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PasswordResetRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, String)> {
    // Always return success to prevent email enumeration
    let _ = state.manager.request_password_reset(&req.email).await;
    Ok(Json(SuccessResponse { success: true }))
}

pub async fn reset_password(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PasswordResetConfirm>,
) -> Result<Json<SuccessResponse>, (StatusCode, String)> {
    if req.new_password.len() < 8 {
        return Err((StatusCode::BAD_REQUEST, "password must be at least 8 characters".to_string()));
    }
    let success = state.manager.reset_password(&req.token, &req.new_password).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(SuccessResponse { success }))
}

pub async fn change_password(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, String)> {
    if req.new_password.len() < 8 {
        return Err((StatusCode::BAD_REQUEST, "password must be at least 8 characters".to_string()));
    }
    let success = state.manager.change_password(&tenant.id.0, &req.current, &req.new_password).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !success {
        return Err((StatusCode::UNAUTHORIZED, "incorrect current password".to_string()));
    }
    Ok(Json(SuccessResponse { success }))
}

// ── Token refresh ──

pub async fn refresh(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
) -> Result<Json<RefreshResponse>, (StatusCode, String)> {
    let token = system_tenants::auth::issue_token(&tenant.id, &state.platform.platform.jwt_secret)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(RefreshResponse { token }))
}

// ── Profile ──

pub async fn profile(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProfileResponse>, (StatusCode, String)> {
    let stats = tenant
        .companion_store
        .collection_stats()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Check if TOTP is enabled
    let totp_enabled = {
        let db = state.manager.db().await;
        db.query_row(
            "SELECT totp_enabled FROM auth WHERE tenant_id = ?1",
            rusqlite::params![tenant.id.0],
            |row| row.get::<_, bool>(0),
        ).unwrap_or(false)
    };

    Ok(Json(ProfileResponse {
        display_name: tenant.display_name.clone(),
        tier: tenant.tier_name.clone(),
        companions_count: stats.total_companions as usize,
        created_at: tenant.created_at,
        email: tenant.email.clone(),
        totp_enabled,
    }))
}

// ── Usage ──

pub async fn usage(
    AuthTenant(tenant): AuthTenant,
) -> Result<Json<UsageResponse>, (StatusCode, String)> {
    let (cost, _, _) = tenant.cost_ledger.budget_status();
    let storage = system_tenants::storage::disk_usage_mb(&tenant.data_dir);
    let stats = tenant
        .companion_store
        .collection_stats()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(UsageResponse {
        cost_today_usd: cost,
        storage_mb: storage,
        companions_count: stats.total_companions as usize,
        tier_limit_companions: tenant.tier.max_companions,
        tier_limit_cost_usd: tenant.tier.max_cost_per_day_usd,
    }))
}

// ── Economy ──

pub async fn economy(
    AuthTenant(tenant): AuthTenant,
    State(state): State<Arc<AppState>>,
) -> Result<Json<EconomyResponse>, (StatusCode, String)> {
    let db = state.manager.db().await;
    let balance = system_tenants::economy::get_balance(&db, &tenant.id.0, &tenant.tier)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(EconomyResponse {
        summons: balance.summons,
        summons_max: balance.summons_max,
        mana: balance.mana,
        mana_max: balance.mana_max,
    }))
}
