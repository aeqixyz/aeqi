use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn, debug};

use crate::auth::{self, SessionToken, LoginResult};
use crate::config::PlatformConfig;
use crate::provision;
use crate::storage;
use crate::tenant::{Tenant, TenantId};

/// Manages all tenants: creation, loading, unloading, lookup.
pub struct TenantManager {
    tenants: RwLock<HashMap<TenantId, Arc<Tenant>>>,
    base_dir: PathBuf,
    template_dir: PathBuf,
    pub platform: PlatformConfig,
    db: tokio::sync::Mutex<rusqlite::Connection>,
}

impl TenantManager {
    /// Create a new TenantManager with a SQLite index DB.
    pub fn new(platform: PlatformConfig) -> Result<Self> {
        let base_dir = platform.base_dir();
        let template_dir = platform.template_dir();
        std::fs::create_dir_all(&base_dir)?;

        let db_path = base_dir.join("tenants.db");
        let conn = rusqlite::Connection::open(&db_path)
            .with_context(|| format!("failed to open tenant DB: {}", db_path.display()))?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;

             CREATE TABLE IF NOT EXISTS tenants (
                 id TEXT PRIMARY KEY,
                 display_name TEXT NOT NULL,
                 email TEXT,
                 tier TEXT NOT NULL DEFAULT 'free',
                 created_at TEXT NOT NULL,
                 stripe_customer_id TEXT,
                 stripe_subscription_id TEXT
             );

             CREATE TABLE IF NOT EXISTS sessions (
                 token_hash TEXT PRIMARY KEY,
                 tenant_id TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 expires_at TEXT,
                 FOREIGN KEY (tenant_id) REFERENCES tenants(id)
             );

             CREATE INDEX IF NOT EXISTS idx_sessions_tenant ON sessions(tenant_id);

             CREATE TABLE IF NOT EXISTS auth (
                 tenant_id TEXT PRIMARY KEY,
                 email TEXT UNIQUE NOT NULL,
                 password_hash TEXT NOT NULL,
                 email_verified INTEGER NOT NULL DEFAULT 0,
                 totp_secret TEXT,
                 totp_enabled INTEGER NOT NULL DEFAULT 0,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL,
                 FOREIGN KEY (tenant_id) REFERENCES tenants(id)
             );
             CREATE UNIQUE INDEX IF NOT EXISTS idx_auth_email ON auth(email);

             CREATE TABLE IF NOT EXISTS email_verifications (
                 token TEXT PRIMARY KEY,
                 tenant_id TEXT NOT NULL,
                 expires_at TEXT NOT NULL,
                 FOREIGN KEY (tenant_id) REFERENCES tenants(id)
             );

             CREATE TABLE IF NOT EXISTS password_resets (
                 token TEXT PRIMARY KEY,
                 tenant_id TEXT NOT NULL,
                 expires_at TEXT NOT NULL,
                 used INTEGER NOT NULL DEFAULT 0,
                 FOREIGN KEY (tenant_id) REFERENCES tenants(id)
             );

             CREATE TABLE IF NOT EXISTS economy (
                 tenant_id TEXT PRIMARY KEY,
                 summons INTEGER NOT NULL DEFAULT 0,
                 mana INTEGER NOT NULL DEFAULT 0,
                 last_regen TEXT NOT NULL,
                 FOREIGN KEY (tenant_id) REFERENCES tenants(id)
             );

             CREATE TABLE IF NOT EXISTS economy_log (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 tenant_id TEXT NOT NULL,
                 currency TEXT NOT NULL,
                 delta INTEGER NOT NULL,
                 reason TEXT NOT NULL,
                 created_at TEXT NOT NULL
             );"
        )?;

        // Add stripe columns if they don't exist (migration for existing DBs)
        let _ = conn.execute("ALTER TABLE tenants ADD COLUMN stripe_customer_id TEXT", []);
        let _ = conn.execute("ALTER TABLE tenants ADD COLUMN stripe_subscription_id TEXT", []);

        Ok(Self {
            tenants: RwLock::new(HashMap::new()),
            base_dir,
            template_dir,
            platform,
            db: tokio::sync::Mutex::new(conn),
        })
    }

    /// Get a reference to the shared DB connection (for economy operations).
    pub async fn db(&self) -> tokio::sync::MutexGuard<'_, rusqlite::Connection> {
        self.db.lock().await
    }

    // ── Anonymous tenant creation (legacy) ──

    /// Create a new tenant (anonymous), provision directories, return token.
    pub async fn create_tenant(&self, display_name: &str) -> Result<(Arc<Tenant>, SessionToken)> {
        let tenant_id = TenantId::new();
        let tier_name = "free".to_string();
        let tier = self.platform.tier(&tier_name);

        // Insert into index DB.
        {
            let db = self.db.lock().await;
            let now = chrono::Utc::now().to_rfc3339();
            db.execute(
                "INSERT INTO tenants (id, display_name, tier, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![tenant_id.0, display_name, tier_name, now],
            )?;
        }

        // Provision on-disk structure from templates.
        let data_dir = self.base_dir.join(&tenant_id.0);
        provision::provision_tenant(&data_dir, &self.template_dir, &tenant_id, display_name, &tier_name)?;

        // Load into memory.
        let tenant = self.load_tenant_from_disk(&tenant_id, &data_dir, &tier, &tier_name, display_name, None).await?;
        let tenant = Arc::new(tenant);

        // Issue JWT.
        let token = auth::issue_token(&tenant_id, &self.platform.platform.jwt_secret)?;

        self.tenants.write().await.insert(tenant_id.clone(), tenant.clone());
        info!(tenant = %tenant_id, name = %display_name, "tenant created");

        Ok((tenant, token))
    }

    // ── Email+password auth ──

    /// Create a new tenant with email+password auth. Returns (tenant, JWT, email verification token).
    pub async fn create_tenant_with_auth(
        &self,
        email: &str,
        password: &str,
        display_name: Option<&str>,
    ) -> Result<(Arc<Tenant>, SessionToken, String)> {
        let display_name = display_name.unwrap_or("Daemon");
        let tenant_id = TenantId::new();
        let tier_name = "free".to_string();
        let tier = self.platform.tier(&tier_name);
        let password_hash = auth::hash_password(password)?;
        let verification_token = auth::generate_verification_token();

        {
            let db = self.db.lock().await;
            let now = chrono::Utc::now().to_rfc3339();
            let expires = (chrono::Utc::now() + chrono::Duration::hours(24)).to_rfc3339();

            db.execute(
                "INSERT INTO tenants (id, display_name, email, tier, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![tenant_id.0, display_name, email, tier_name, now],
            )?;

            db.execute(
                "INSERT INTO auth (tenant_id, email, password_hash, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![tenant_id.0, email, password_hash, now, now],
            )?;

            db.execute(
                "INSERT INTO email_verifications (token, tenant_id, expires_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![verification_token, tenant_id.0, expires],
            )?;
        }

        // Provision on-disk structure.
        let data_dir = self.base_dir.join(&tenant_id.0);
        provision::provision_tenant(&data_dir, &self.template_dir, &tenant_id, display_name, &tier_name)?;

        // Load into memory.
        let tenant = self.load_tenant_from_disk(&tenant_id, &data_dir, &tier, &tier_name, display_name, Some(email)).await?;
        let tenant = Arc::new(tenant);

        // Issue JWT.
        let token = auth::issue_token(&tenant_id, &self.platform.platform.jwt_secret)?;

        self.tenants.write().await.insert(tenant_id.clone(), tenant.clone());
        info!(tenant = %tenant_id, email = %email, "tenant created with auth");

        Ok((tenant, token, verification_token))
    }

    /// Login with email + password. Returns LoginResult.
    pub async fn login(&self, email: &str, password: &str) -> Result<LoginResult> {
        let db = self.db.lock().await;

        // Look up auth record by email.
        let row = db.query_row(
            "SELECT tenant_id, password_hash, email_verified, totp_enabled, totp_secret FROM auth WHERE email = ?1",
            rusqlite::params![email],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        );

        let (tenant_id_str, password_hash, email_verified, totp_enabled, _totp_secret) = match row {
            Ok(r) => r,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(LoginResult::InvalidCredentials),
            Err(e) => return Err(e.into()),
        };

        // Verify password.
        if !auth::verify_password(password, &password_hash)? {
            return Ok(LoginResult::InvalidCredentials);
        }

        // Check email verification.
        if !email_verified {
            return Ok(LoginResult::EmailNotVerified);
        }

        // Check TOTP.
        if totp_enabled {
            return Ok(LoginResult::RequiresTOTP(tenant_id_str));
        }

        let tenant_id = TenantId(tenant_id_str);
        let token = auth::issue_token(&tenant_id, &self.platform.platform.jwt_secret)?;
        Ok(LoginResult::Success(token))
    }

    /// Complete TOTP login step.
    pub async fn verify_totp_login(&self, tenant_id: &str, code: &str) -> Result<Option<SessionToken>> {
        let db = self.db.lock().await;

        let secret: Option<String> = db.query_row(
            "SELECT totp_secret FROM auth WHERE tenant_id = ?1 AND totp_enabled = 1",
            rusqlite::params![tenant_id],
            |row| row.get(0),
        ).ok();

        if let Some(secret) = secret
            && auth::verify_totp(&secret, code)?
        {
            let tid = TenantId(tenant_id.to_string());
            let token = auth::issue_token(&tid, &self.platform.platform.jwt_secret)?;
            return Ok(Some(token));
        }

        Ok(None)
    }

    /// Verify an email using the verification token.
    pub async fn verify_email(&self, token: &str) -> Result<bool> {
        let db = self.db.lock().await;
        let now = chrono::Utc::now().to_rfc3339();

        let row = db.query_row(
            "SELECT tenant_id, expires_at FROM email_verifications WHERE token = ?1",
            rusqlite::params![token],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        );

        let (tenant_id, expires_at) = match row {
            Ok(r) => r,
            Err(_) => return Ok(false),
        };

        // Check expiration.
        if now > expires_at {
            return Ok(false);
        }

        // Mark email as verified.
        db.execute(
            "UPDATE auth SET email_verified = 1, updated_at = ?1 WHERE tenant_id = ?2",
            rusqlite::params![now, tenant_id],
        )?;

        // Delete used token.
        db.execute("DELETE FROM email_verifications WHERE token = ?1", rusqlite::params![token])?;

        Ok(true)
    }

    /// Setup TOTP for a tenant. Returns (secret_base32, otpauth_uri).
    pub async fn setup_totp(&self, tenant_id: &str) -> Result<(String, String)> {
        let db = self.db.lock().await;

        let email: String = db.query_row(
            "SELECT email FROM auth WHERE tenant_id = ?1",
            rusqlite::params![tenant_id],
            |row| row.get(0),
        ).context("tenant auth not found")?;

        let (secret, uri) = auth::generate_totp_secret(&email)?;

        // Store the secret (not yet enabled).
        let now = chrono::Utc::now().to_rfc3339();
        db.execute(
            "UPDATE auth SET totp_secret = ?1, updated_at = ?2 WHERE tenant_id = ?3",
            rusqlite::params![secret, now, tenant_id],
        )?;

        Ok((secret, uri))
    }

    /// Enable TOTP after verifying a code. Returns true if code is valid.
    pub async fn enable_totp(&self, tenant_id: &str, code: &str) -> Result<bool> {
        let db = self.db.lock().await;

        let secret: Option<String> = db.query_row(
            "SELECT totp_secret FROM auth WHERE tenant_id = ?1",
            rusqlite::params![tenant_id],
            |row| row.get(0),
        ).ok().flatten();

        let Some(secret) = secret else { return Ok(false) };

        if !auth::verify_totp(&secret, code)? {
            return Ok(false);
        }

        let now = chrono::Utc::now().to_rfc3339();
        db.execute(
            "UPDATE auth SET totp_enabled = 1, updated_at = ?1 WHERE tenant_id = ?2",
            rusqlite::params![now, tenant_id],
        )?;

        Ok(true)
    }

    /// Disable TOTP (requires current code).
    pub async fn disable_totp(&self, tenant_id: &str, code: &str) -> Result<bool> {
        let db = self.db.lock().await;

        let secret: Option<String> = db.query_row(
            "SELECT totp_secret FROM auth WHERE tenant_id = ?1 AND totp_enabled = 1",
            rusqlite::params![tenant_id],
            |row| row.get(0),
        ).ok();

        let Some(secret) = secret else { return Ok(false) };

        if !auth::verify_totp(&secret, code)? {
            return Ok(false);
        }

        let now = chrono::Utc::now().to_rfc3339();
        db.execute(
            "UPDATE auth SET totp_enabled = 0, totp_secret = NULL, updated_at = ?1 WHERE tenant_id = ?2",
            rusqlite::params![now, tenant_id],
        )?;

        Ok(true)
    }

    /// Request a password reset. Returns the reset token if the email exists.
    pub async fn request_password_reset(&self, email: &str) -> Result<Option<String>> {
        let db = self.db.lock().await;

        let tenant_id: Option<String> = db.query_row(
            "SELECT tenant_id FROM auth WHERE email = ?1",
            rusqlite::params![email],
            |row| row.get(0),
        ).ok();

        let Some(tenant_id) = tenant_id else { return Ok(None) };

        let token = auth::generate_verification_token();
        let expires = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();

        db.execute(
            "INSERT INTO password_resets (token, tenant_id, expires_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![token, tenant_id, expires],
        )?;

        Ok(Some(token))
    }

    /// Reset password using a reset token.
    pub async fn reset_password(&self, token: &str, new_password: &str) -> Result<bool> {
        let db = self.db.lock().await;
        let now = chrono::Utc::now().to_rfc3339();

        let row = db.query_row(
            "SELECT tenant_id, expires_at, used FROM password_resets WHERE token = ?1",
            rusqlite::params![token],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, bool>(2)?)),
        );

        let (tenant_id, expires_at, used) = match row {
            Ok(r) => r,
            Err(_) => return Ok(false),
        };

        if used || now > expires_at {
            return Ok(false);
        }

        let password_hash = auth::hash_password(new_password)?;

        db.execute(
            "UPDATE auth SET password_hash = ?1, updated_at = ?2 WHERE tenant_id = ?3",
            rusqlite::params![password_hash, now, tenant_id],
        )?;

        db.execute(
            "UPDATE password_resets SET used = 1 WHERE token = ?1",
            rusqlite::params![token],
        )?;

        Ok(true)
    }

    /// Change password (requires current password).
    pub async fn change_password(&self, tenant_id: &str, current: &str, new_password: &str) -> Result<bool> {
        let db = self.db.lock().await;

        let hash: String = db.query_row(
            "SELECT password_hash FROM auth WHERE tenant_id = ?1",
            rusqlite::params![tenant_id],
            |row| row.get(0),
        ).context("auth not found")?;

        if !auth::verify_password(current, &hash)? {
            return Ok(false);
        }

        let new_hash = auth::hash_password(new_password)?;
        let now = chrono::Utc::now().to_rfc3339();
        db.execute(
            "UPDATE auth SET password_hash = ?1, updated_at = ?2 WHERE tenant_id = ?3",
            rusqlite::params![new_hash, now, tenant_id],
        )?;

        Ok(true)
    }

    // ── Stripe ──

    /// Update a tenant's tier.
    pub async fn update_tier(&self, tenant_id: &str, tier: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "UPDATE tenants SET tier = ?1 WHERE id = ?2",
            rusqlite::params![tier, tenant_id],
        )?;

        // In-memory tenant will pick up new tier on next load (Arc prevents in-place mutation).
        drop(db);

        Ok(())
    }

    /// Set Stripe customer ID for a tenant.
    pub async fn set_stripe_customer(&self, tenant_id: &str, customer_id: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "UPDATE tenants SET stripe_customer_id = ?1 WHERE id = ?2",
            rusqlite::params![customer_id, tenant_id],
        )?;
        Ok(())
    }

    /// Set Stripe subscription ID for a tenant.
    pub async fn set_stripe_subscription(&self, tenant_id: &str, subscription_id: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.execute(
            "UPDATE tenants SET stripe_subscription_id = ?1 WHERE id = ?2",
            rusqlite::params![subscription_id, tenant_id],
        )?;
        Ok(())
    }

    /// Get Stripe customer ID for a tenant.
    pub async fn get_stripe_customer(&self, tenant_id: &str) -> Result<Option<String>> {
        let db = self.db.lock().await;
        let result = db.query_row(
            "SELECT stripe_customer_id FROM tenants WHERE id = ?1",
            rusqlite::params![tenant_id],
            |row| row.get(0),
        ).ok();
        Ok(result)
    }

    /// Get the email for a tenant from the auth table.
    pub async fn get_tenant_email(&self, tenant_id: &str) -> Result<Option<String>> {
        let db = self.db.lock().await;
        let result = db.query_row(
            "SELECT email FROM auth WHERE tenant_id = ?1",
            rusqlite::params![tenant_id],
            |row| row.get(0),
        ).ok();
        Ok(result)
    }

    // ── Core tenant operations ──

    /// Look up a tenant by session token (JWT).
    pub async fn resolve_by_session(&self, token: &str) -> Result<Option<Arc<Tenant>>> {
        let claims = auth::validate_token(token, &self.platform.platform.jwt_secret)?;
        let tenant_id = TenantId(claims.sub);

        // Check if already loaded.
        if let Some(t) = self.tenants.read().await.get(&tenant_id) {
            t.touch();
            return Ok(Some(t.clone()));
        }

        // Try to load from disk.
        let data_dir = self.base_dir.join(&tenant_id.0);
        if !data_dir.exists() {
            return Ok(None);
        }

        match self.load_tenant(&tenant_id).await {
            Ok(t) => {
                t.touch();
                Ok(Some(t))
            }
            Err(e) => {
                warn!(tenant = %tenant_id, error = %e, "failed to load tenant");
                Ok(None)
            }
        }
    }

    /// Load a tenant from disk into memory.
    pub async fn load_tenant(&self, tenant_id: &TenantId) -> Result<Arc<Tenant>> {
        let data_dir = self.base_dir.join(&tenant_id.0);
        let meta = storage::load_tenant_meta(&data_dir)?;
        let tier_name = meta.tier.clone();
        let tier = self.platform.tier(&tier_name);

        let tenant = self.load_tenant_from_disk(
            tenant_id, &data_dir, &tier, &tier_name,
            &meta.display_name, meta.email.as_deref(),
        ).await?;
        let tenant = Arc::new(tenant);

        self.tenants.write().await.insert(tenant_id.clone(), tenant.clone());
        debug!(tenant = %tenant_id, "tenant loaded");
        Ok(tenant)
    }

    /// Unload tenants that have been idle for longer than threshold.
    pub async fn unload_idle(&self, threshold: Duration) {
        let threshold_secs = threshold.as_secs();
        let mut tenants = self.tenants.write().await;
        let before = tenants.len();
        tenants.retain(|id, t| {
            let idle = t.idle_secs();
            if idle > threshold_secs {
                debug!(tenant = %id, idle_secs = idle, "unloading idle tenant");
                false
            } else {
                true
            }
        });
        let removed = before - tenants.len();
        if removed > 0 {
            info!(removed = removed, remaining = tenants.len(), "unloaded idle tenants");
        }
    }

    /// Load all tenants from disk (startup scan).
    pub async fn load_all(&self) -> Result<usize> {
        let entries = std::fs::read_dir(&self.base_dir)?;
        let mut count = 0;
        for entry in entries {
            let entry = entry?;
            if !entry.path().is_dir() { continue; }
            let name = entry.file_name().to_string_lossy().to_string();
            // Skip non-UUID directories.
            if name.len() < 32 { continue; }
            let tenant_id = TenantId(name);
            match self.load_tenant(&tenant_id).await {
                Ok(_) => count += 1,
                Err(e) => warn!(tenant = %tenant_id, error = %e, "failed to load tenant on startup"),
            }
        }
        info!(loaded = count, "startup tenant scan complete");
        Ok(count)
    }

    /// Get all active tenants.
    pub async fn active_tenants(&self) -> Vec<Arc<Tenant>> {
        self.tenants.read().await.values().cloned().collect()
    }

    /// Count of loaded tenants.
    pub async fn active_count(&self) -> usize {
        self.tenants.read().await.len()
    }

    /// Sum of daily cost across all tenants.
    pub async fn global_cost(&self) -> f64 {
        let tenants = self.tenants.read().await;
        tenants.values().map(|t| {
            let (spent, _, _) = t.cost_ledger.budget_status();
            spent
        }).sum()
    }

    /// Internal: hydrate a Tenant struct from disk paths.
    async fn load_tenant_from_disk(
        &self,
        tenant_id: &TenantId,
        data_dir: &std::path::Path,
        tier: &crate::config::TierConfig,
        tier_name: &str,
        display_name: &str,
        email: Option<&str>,
    ) -> Result<Tenant> {
        use system_companions::CompanionStore;
        use system_orchestrator::{ProjectRegistry, ConversationStore, CostLedger, DispatchBus};

        let dispatch_bus = Arc::new(DispatchBus::with_persistence(data_dir.join("whispers")));
        let cost_ledger = Arc::new(CostLedger::new(tier.max_cost_per_day_usd));
        let companion_store = Arc::new(CompanionStore::open(&data_dir.join("companions.db"))?);
        let conversation_store = Arc::new(ConversationStore::open(&data_dir.join("conversations.db"))?);
        let registry = Arc::new(ProjectRegistry::new(dispatch_bus.clone(), "system".to_string()));

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(Tenant {
            id: tenant_id.clone(),
            display_name: display_name.to_string(),
            email: email.map(|s| s.to_string()),
            tier: tier.clone(),
            tier_name: tier_name.to_string(),
            data_dir: data_dir.to_path_buf(),
            registry,
            dispatch_bus,
            cost_ledger,
            companion_store,
            conversation_store,
            last_active: std::sync::atomic::AtomicU64::new(now),
            created_at: chrono::Utc::now(),
        })
    }
}
