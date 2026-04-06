use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: Option<String>,
    pub name: String,
    pub avatar_url: Option<String>,
    pub provider: String,
    pub provider_id: Option<String>,
    pub created_at: String,
    pub stripe_customer_id: Option<String>,
    pub subscription_status: String,
    pub subscription_plan: Option<String>,
    pub stripe_subscription_id: Option<String>,
    pub trial_ends_at: Option<String>,
}

pub struct UserStore {
    db: Mutex<Connection>,
}

impl UserStore {
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                email TEXT NOT NULL UNIQUE,
                password_hash TEXT,
                name TEXT NOT NULL DEFAULT '',
                avatar_url TEXT,
                provider TEXT NOT NULL DEFAULT 'local',
                provider_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_users_provider
                ON users(provider, provider_id) WHERE provider_id IS NOT NULL;

            CREATE TABLE IF NOT EXISTS users_companies (
                user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                company_name TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'owner',
                created_at TEXT NOT NULL,
                PRIMARY KEY (user_id, company_name)
            );

            CREATE TABLE IF NOT EXISTS oauth_states (
                state TEXT PRIMARY KEY,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS email_verifications (
                email TEXT PRIMARY KEY,
                code TEXT NOT NULL,
                user_id TEXT,
                created_at TEXT NOT NULL
            );",
        )?;

        // Idempotent migration: add email_verified column.
        let has_verified: bool = conn
            .prepare("PRAGMA table_info(users)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .any(|col| col == "email_verified");
        if !has_verified {
            conn.execute_batch(
                "ALTER TABLE users ADD COLUMN email_verified INTEGER NOT NULL DEFAULT 1;",
            )?;
        }

        // Idempotent migration: add attempts column to email_verifications.
        {
            let has_attempts: bool = conn
                .prepare("PRAGMA table_info(email_verifications)")?
                .query_map([], |row| row.get::<_, String>(1))?
                .filter_map(|r| r.ok())
                .any(|col| col == "attempts");
            if !has_attempts {
                conn.execute_batch(
                    "ALTER TABLE email_verifications ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;",
                )?;
            }
        }

        // Idempotent migration: add Stripe/subscription columns.
        {
            let cols: Vec<String> = conn
                .prepare("PRAGMA table_info(users)")?
                .query_map([], |row| row.get::<_, String>(1))?
                .filter_map(|r| r.ok())
                .collect();
            if !cols.iter().any(|c| c == "stripe_customer_id") {
                conn.execute_batch("ALTER TABLE users ADD COLUMN stripe_customer_id TEXT;")?;
            }
            if !cols.iter().any(|c| c == "subscription_status") {
                conn.execute_batch(
                    "ALTER TABLE users ADD COLUMN subscription_status TEXT NOT NULL DEFAULT 'trialing';",
                )?;
            }
            if !cols.iter().any(|c| c == "subscription_plan") {
                conn.execute_batch("ALTER TABLE users ADD COLUMN subscription_plan TEXT;")?;
            }
            if !cols.iter().any(|c| c == "stripe_subscription_id") {
                conn.execute_batch("ALTER TABLE users ADD COLUMN stripe_subscription_id TEXT;")?;
            }
            if !cols.iter().any(|c| c == "trial_ends_at") {
                conn.execute_batch("ALTER TABLE users ADD COLUMN trial_ends_at TEXT;")?;
            }
        }

        Ok(Self {
            db: Mutex::new(conn),
        })
    }

    pub fn create_user(&self, email: &str, password: &str, name: &str) -> Result<User> {
        use argon2::{
            Argon2, PasswordHasher, password_hash::SaltString, password_hash::rand_core::OsRng,
        };

        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("password hash failed: {e}"))?
            .to_string();

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let trial_ends = (chrono::Utc::now() + chrono::Duration::days(7)).to_rfc3339();

        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO users (id, email, password_hash, name, provider, subscription_status, trial_ends_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'local', 'trialing', ?5, ?6, ?6)",
            rusqlite::params![id, email, hash, name, trial_ends, now],
        )?;

        Ok(User {
            id,
            email: email.to_string(),
            password_hash: Some(hash),
            name: name.to_string(),
            avatar_url: None,
            provider: "local".to_string(),
            provider_id: None,
            created_at: now,
            stripe_customer_id: None,
            subscription_status: "trialing".to_string(),
            subscription_plan: None,
            stripe_subscription_id: None,
            trial_ends_at: Some(trial_ends),
        })
    }

    pub fn find_by_email(&self, email: &str) -> Option<User> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT id, email, password_hash, name, avatar_url, provider, provider_id, created_at,
                    stripe_customer_id, subscription_status, subscription_plan, stripe_subscription_id, trial_ends_at
             FROM users WHERE email = ?1",
            rusqlite::params![email],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    password_hash: row.get(2)?,
                    name: row.get(3)?,
                    avatar_url: row.get(4)?,
                    provider: row.get(5)?,
                    provider_id: row.get(6)?,
                    created_at: row.get(7)?,
                    stripe_customer_id: row.get(8)?,
                    subscription_status: row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "trialing".to_string()),
                    subscription_plan: row.get(10)?,
                    stripe_subscription_id: row.get(11)?,
                    trial_ends_at: row.get(12)?,
                })
            },
        )
        .ok()
    }

    pub fn find_by_id(&self, id: &str) -> Option<User> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT id, email, password_hash, name, avatar_url, provider, provider_id, created_at,
                    stripe_customer_id, subscription_status, subscription_plan, stripe_subscription_id, trial_ends_at
             FROM users WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    password_hash: row.get(2)?,
                    name: row.get(3)?,
                    avatar_url: row.get(4)?,
                    provider: row.get(5)?,
                    provider_id: row.get(6)?,
                    created_at: row.get(7)?,
                    stripe_customer_id: row.get(8)?,
                    subscription_status: row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "trialing".to_string()),
                    subscription_plan: row.get(10)?,
                    stripe_subscription_id: row.get(11)?,
                    trial_ends_at: row.get(12)?,
                })
            },
        )
        .ok()
    }

    pub fn find_or_create_oauth(
        &self,
        email: &str,
        name: &str,
        avatar: &str,
        provider: &str,
        provider_id: &str,
    ) -> User {
        // Try by email first.
        if let Some(user) = self.find_by_email(email) {
            // Security: if the existing user is a local account with a password,
            // do NOT auto-link the OAuth provider. The user must log in with their
            // password first and link OAuth from settings. Return the existing user
            // but don't overwrite their provider_id.
            if user.provider == "local" && user.password_hash.is_some() {
                // Only update avatar if not set (cosmetic, not a security field).
                if user.avatar_url.is_none() && !avatar.is_empty() {
                    let db = self.db.lock().unwrap();
                    if let Err(e) = db.execute(
                        "UPDATE users SET avatar_url = ?1 WHERE id = ?2",
                        rusqlite::params![avatar, user.id],
                    ) {
                        tracing::error!(error = %e, "failed to update avatar for local user");
                    }
                }
                return user;
            }
            // For OAuth-created accounts (no password), safe to update provider info.
            if user.avatar_url.is_none() && !avatar.is_empty() {
                let db = self.db.lock().unwrap();
                if let Err(e) = db.execute(
                    "UPDATE users SET avatar_url = ?1, provider_id = ?2 WHERE id = ?3",
                    rusqlite::params![avatar, provider_id, user.id],
                ) {
                    tracing::error!(error = %e, "failed to update avatar/provider for OAuth user");
                }
            }
            return user;
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let trial_ends = (chrono::Utc::now() + chrono::Duration::days(7)).to_rfc3339();
        let avatar_opt = if avatar.is_empty() {
            None
        } else {
            Some(avatar.to_string())
        };

        let db = self.db.lock().unwrap();
        if let Err(e) = db.execute(
            "INSERT INTO users (id, email, name, avatar_url, provider, provider_id, subscription_status, trial_ends_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'trialing', ?7, ?8, ?8)",
            rusqlite::params![id, email, name, avatar_opt, provider, provider_id, trial_ends, now],
        ) {
            tracing::error!(error = %e, email = %email, provider = %provider, "failed to create OAuth user");
        }

        User {
            id,
            email: email.to_string(),
            password_hash: None,
            name: name.to_string(),
            avatar_url: avatar_opt,
            provider: provider.to_string(),
            provider_id: Some(provider_id.to_string()),
            created_at: now,
            stripe_customer_id: None,
            subscription_status: "trialing".to_string(),
            subscription_plan: None,
            stripe_subscription_id: None,
            trial_ends_at: Some(trial_ends),
        }
    }

    pub fn verify_password(&self, user: &User, password: &str) -> bool {
        use argon2::{Argon2, PasswordHash, PasswordVerifier};

        let Some(ref hash) = user.password_hash else {
            return false;
        };
        let Ok(parsed) = PasswordHash::new(hash) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    }

    pub fn get_user_companies(&self, user_id: &str) -> Vec<String> {
        let db = self.db.lock().unwrap();
        let mut stmt = db
            .prepare("SELECT company_name FROM users_companies WHERE user_id = ?1")
            .unwrap();
        stmt.query_map(rusqlite::params![user_id], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    pub fn add_user_company(&self, user_id: &str, company_name: &str, role: &str) {
        let db = self.db.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        if let Err(e) = db.execute(
            "INSERT OR IGNORE INTO users_companies (user_id, company_name, role, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![user_id, company_name, role, now],
        ) {
            tracing::error!(error = %e, user_id = %user_id, company = %company_name, "failed to add user company membership");
        }
    }

    pub fn save_oauth_state(&self, state: &str) {
        let db = self.db.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        if let Err(e) = db.execute(
            "INSERT INTO oauth_states (state, created_at) VALUES (?1, ?2)",
            rusqlite::params![state, now],
        ) {
            tracing::error!(error = %e, "failed to save OAuth state");
        }
        // Clean up old states (> 10 min).
        if let Err(e) = db.execute(
            "DELETE FROM oauth_states WHERE created_at < datetime('now', '-10 minutes')",
            [],
        ) {
            tracing::error!(error = %e, "failed to clean up expired OAuth states");
        }
    }

    pub fn consume_oauth_state(&self, state: &str) -> bool {
        let db = self.db.lock().unwrap();
        let deleted = db
            .execute(
                "DELETE FROM oauth_states WHERE state = ?1",
                rusqlite::params![state],
            )
            .unwrap_or(0);
        deleted > 0
    }

    // -- Email verification ---------------------------------------------------

    /// Create user with email_verified = false (for email verification flow).
    pub fn create_user_unverified(&self, email: &str, password: &str, name: &str) -> Result<User> {
        use argon2::{
            Argon2, PasswordHasher, password_hash::SaltString, password_hash::rand_core::OsRng,
        };

        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("password hash failed: {e}"))?
            .to_string();

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let trial_ends = (chrono::Utc::now() + chrono::Duration::days(7)).to_rfc3339();

        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO users (id, email, password_hash, name, provider, email_verified, subscription_status, trial_ends_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'local', 0, 'trialing', ?5, ?6, ?6)",
            rusqlite::params![id, email, hash, name, trial_ends, now],
        )?;

        Ok(User {
            id,
            email: email.to_string(),
            password_hash: Some(hash),
            name: name.to_string(),
            avatar_url: None,
            provider: "local".to_string(),
            provider_id: None,
            created_at: now,
            stripe_customer_id: None,
            subscription_status: "trialing".to_string(),
            subscription_plan: None,
            stripe_subscription_id: None,
            trial_ends_at: Some(trial_ends),
        })
    }

    /// Generate and store a 6-digit verification code.
    pub fn create_verification_code(&self, email: &str, user_id: &str) -> String {
        use argon2::password_hash::rand_core::{OsRng, RngCore};
        let code = format!("{:06}", OsRng.next_u32() % 1_000_000);
        let now = chrono::Utc::now().to_rfc3339();
        let db = self.db.lock().unwrap();
        if let Err(e) = db.execute(
            "INSERT OR REPLACE INTO email_verifications (email, code, user_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![email, code, user_id, now],
        ) {
            tracing::error!(error = %e, email = %email, "failed to save email verification code");
        }
        // Clean up expired codes (> 10 min).
        if let Err(e) = db.execute(
            "DELETE FROM email_verifications WHERE created_at < datetime('now', '-10 minutes')",
            [],
        ) {
            tracing::error!(error = %e, "failed to clean up expired verification codes");
        }
        code
    }

    /// Verify code and mark user as verified. Returns user if valid.
    /// Uses a transaction to ensure check + update + delete are atomic.
    pub fn verify_email(&self, email: &str, code: &str) -> Option<User> {
        let mut db = self.db.lock().unwrap();

        // Check attempt count before doing anything.
        let attempts: i32 = db
            .query_row(
                "SELECT COALESCE(attempts, 0) FROM email_verifications WHERE email = ?1",
                rusqlite::params![email],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if attempts >= 5 {
            // Too many failed attempts — delete the record and force resend.
            let _ = db.execute(
                "DELETE FROM email_verifications WHERE email = ?1",
                rusqlite::params![email],
            );
            return None;
        }

        // Check code matches.
        let stored: Option<(String, String)> = db
            .query_row(
                "SELECT code, user_id FROM email_verifications WHERE email = ?1",
                rusqlite::params![email],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let (stored_code, user_id) = stored?;

        if stored_code != code {
            // Increment attempt counter.
            let _ = db.execute(
                "UPDATE email_verifications SET attempts = COALESCE(attempts, 0) + 1 WHERE email = ?1",
                rusqlite::params![email],
            );
            // Re-check if this attempt hit the limit.
            let new_attempts: i32 = db
                .query_row(
                    "SELECT COALESCE(attempts, 0) FROM email_verifications WHERE email = ?1",
                    rusqlite::params![email],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            if new_attempts >= 5 {
                let _ = db.execute(
                    "DELETE FROM email_verifications WHERE email = ?1",
                    rusqlite::params![email],
                );
            }
            return None;
        }

        // Wrap update + delete in a transaction (unchecked since we hold the Mutex).
        let tx = match db.transaction() {
            Ok(tx) => tx,
            Err(_) => return None,
        };

        let now = chrono::Utc::now().to_rfc3339();
        if tx
            .execute(
                "UPDATE users SET email_verified = 1, updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, user_id],
            )
            .is_err()
        {
            return None;
        }

        if tx
            .execute(
                "DELETE FROM email_verifications WHERE email = ?1",
                rusqlite::params![email],
            )
            .is_err()
        {
            return None;
        }

        if tx.commit().is_err() {
            return None;
        }

        // Return the user.
        drop(db);
        self.find_by_email(email)
    }

    /// Check if a resend is allowed (rate limit: 1 per 60s).
    pub fn can_resend_code(&self, email: &str) -> bool {
        let db = self.db.lock().unwrap();
        let recent: bool = db
            .query_row(
                "SELECT COUNT(*) FROM email_verifications
                 WHERE email = ?1 AND created_at > datetime('now', '-60 seconds')",
                rusqlite::params![email],
                |row| row.get::<_, i32>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        !recent
    }

    /// Check if user's email is verified.
    pub fn is_email_verified(&self, user_id: &str) -> bool {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT email_verified FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |row| row.get::<_, i32>(0),
        )
        .map(|v| v == 1)
        .unwrap_or(false)
    }

    // -- Stripe / Subscription ---------------------------------------------------

    pub fn set_stripe_customer(&self, user_id: &str, customer_id: &str) {
        let db = self.db.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        if let Err(e) = db.execute(
            "UPDATE users SET stripe_customer_id = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![customer_id, now, user_id],
        ) {
            tracing::error!(error = %e, user_id = %user_id, "failed to set Stripe customer ID");
        }
    }

    pub fn set_subscription(
        &self,
        user_id: &str,
        status: &str,
        plan: Option<&str>,
        subscription_id: Option<&str>,
    ) {
        let db = self.db.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        if let Err(e) = db.execute(
            "UPDATE users SET subscription_status = ?1, subscription_plan = ?2, stripe_subscription_id = ?3, updated_at = ?4 WHERE id = ?5",
            rusqlite::params![status, plan, subscription_id, now, user_id],
        ) {
            tracing::error!(error = %e, user_id = %user_id, status = %status, "failed to update subscription");
        }
    }

    /// Returns (status, plan, trial_ends_at).
    pub fn get_subscription_status(
        &self,
        user_id: &str,
    ) -> Option<(String, Option<String>, Option<String>)> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT subscription_status, subscription_plan, trial_ends_at FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?
                        .unwrap_or_else(|| "trialing".to_string()),
                    row.get(1)?,
                    row.get(2)?,
                ))
            },
        )
        .ok()
    }

    pub fn find_by_stripe_customer(&self, customer_id: &str) -> Option<User> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT id, email, password_hash, name, avatar_url, provider, provider_id, created_at,
                    stripe_customer_id, subscription_status, subscription_plan, stripe_subscription_id, trial_ends_at
             FROM users WHERE stripe_customer_id = ?1",
            rusqlite::params![customer_id],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    password_hash: row.get(2)?,
                    name: row.get(3)?,
                    avatar_url: row.get(4)?,
                    provider: row.get(5)?,
                    provider_id: row.get(6)?,
                    created_at: row.get(7)?,
                    stripe_customer_id: row.get(8)?,
                    subscription_status: row.get::<_, Option<String>>(9)?
                        .unwrap_or_else(|| "trialing".to_string()),
                    subscription_plan: row.get(10)?,
                    stripe_subscription_id: row.get(11)?,
                    trial_ends_at: row.get(12)?,
                })
            },
        )
        .ok()
    }

    /// Returns true if status is "active" or "trialing" with trial_ends_at in the future.
    pub fn is_subscription_active(&self, user_id: &str) -> bool {
        let Some((status, _, trial_ends)) = self.get_subscription_status(user_id) else {
            return false;
        };
        match status.as_str() {
            "active" => true,
            "trialing" => {
                if let Some(ends) = trial_ends {
                    chrono::DateTime::parse_from_rfc3339(&ends)
                        .map(|dt| dt > chrono::Utc::now())
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}
