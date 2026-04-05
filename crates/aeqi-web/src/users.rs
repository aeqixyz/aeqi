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

        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO users (id, email, password_hash, name, provider, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'local', ?5, ?5)",
            rusqlite::params![id, email, hash, name, now],
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
        })
    }

    pub fn find_by_email(&self, email: &str) -> Option<User> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT id, email, password_hash, name, avatar_url, provider, provider_id, created_at
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
                })
            },
        )
        .ok()
    }

    pub fn find_by_id(&self, id: &str) -> Option<User> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT id, email, password_hash, name, avatar_url, provider, provider_id, created_at
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
                    let _ = db.execute(
                        "UPDATE users SET avatar_url = ?1 WHERE id = ?2",
                        rusqlite::params![avatar, user.id],
                    );
                }
                return user;
            }
            // For OAuth-created accounts (no password), safe to update provider info.
            if user.avatar_url.is_none() && !avatar.is_empty() {
                let db = self.db.lock().unwrap();
                let _ = db.execute(
                    "UPDATE users SET avatar_url = ?1, provider_id = ?2 WHERE id = ?3",
                    rusqlite::params![avatar, provider_id, user.id],
                );
            }
            return user;
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let avatar_opt = if avatar.is_empty() {
            None
        } else {
            Some(avatar.to_string())
        };

        let db = self.db.lock().unwrap();
        let _ = db.execute(
            "INSERT INTO users (id, email, name, avatar_url, provider, provider_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            rusqlite::params![id, email, name, avatar_opt, provider, provider_id, now],
        );

        User {
            id,
            email: email.to_string(),
            password_hash: None,
            name: name.to_string(),
            avatar_url: avatar_opt,
            provider: provider.to_string(),
            provider_id: Some(provider_id.to_string()),
            created_at: now,
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
        let _ = db.execute(
            "INSERT OR IGNORE INTO users_companies (user_id, company_name, role, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![user_id, company_name, role, now],
        );
    }

    pub fn save_oauth_state(&self, state: &str) {
        let db = self.db.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let _ = db.execute(
            "INSERT INTO oauth_states (state, created_at) VALUES (?1, ?2)",
            rusqlite::params![state, now],
        );
        // Clean up old states (> 10 min).
        let _ = db.execute(
            "DELETE FROM oauth_states WHERE created_at < datetime('now', '-10 minutes')",
            [],
        );
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

        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO users (id, email, password_hash, name, provider, email_verified, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'local', 0, ?5, ?5)",
            rusqlite::params![id, email, hash, name, now],
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
        })
    }

    /// Generate and store a 6-digit verification code.
    pub fn create_verification_code(&self, email: &str, user_id: &str) -> String {
        use argon2::password_hash::rand_core::{OsRng, RngCore};
        let code = format!("{:06}", OsRng.next_u32() % 1_000_000);
        let now = chrono::Utc::now().to_rfc3339();
        let db = self.db.lock().unwrap();
        let _ = db.execute(
            "INSERT OR REPLACE INTO email_verifications (email, code, user_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![email, code, user_id, now],
        );
        // Clean up expired codes (> 10 min).
        let _ = db.execute(
            "DELETE FROM email_verifications WHERE created_at < datetime('now', '-10 minutes')",
            [],
        );
        code
    }

    /// Verify code and mark user as verified. Returns user if valid.
    /// Uses a transaction to ensure check + update + delete are atomic.
    pub fn verify_email(&self, email: &str, code: &str) -> Option<User> {
        let mut db = self.db.lock().unwrap();
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
}
