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
            );",
        )?;

        Ok(Self {
            db: Mutex::new(conn),
        })
    }

    pub fn create_user(&self, email: &str, password: &str, name: &str) -> Result<User> {
        use argon2::{Argon2, PasswordHasher, password_hash::SaltString, password_hash::rand_core::OsRng};

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
        // Try by email first (may have signed up with password, now linking Google).
        if let Some(user) = self.find_by_email(email) {
            // Update avatar if not set.
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
        use argon2::{Argon2, PasswordVerifier, PasswordHash};

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
}
