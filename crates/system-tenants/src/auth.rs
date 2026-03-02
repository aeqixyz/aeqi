use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::tenant::TenantId;

/// A JWT session token string.
pub type SessionToken = String;

/// JWT claims.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,       // tenant_id
    pub exp: i64,          // expiration timestamp
    pub iat: i64,          // issued at
    #[serde(default)]
    pub role: Option<String>,  // "admin" for admin users
}

/// Result of a login attempt.
#[derive(Debug)]
pub enum LoginResult {
    Success(SessionToken),
    RequiresTOTP(String),  // tenant_id — needs TOTP code
    InvalidCredentials,
    EmailNotVerified,
}

/// Issue a JWT for a tenant.
pub fn issue_token(tenant_id: &TenantId, secret: &str) -> Result<SessionToken> {
    let now = Utc::now();
    let exp = now + Duration::days(30);
    let claims = Claims {
        sub: tenant_id.0.clone(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        role: None,
    };
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    ).context("failed to encode JWT")?;
    Ok(token)
}

/// Validate and decode a JWT, returning claims.
pub fn validate_token(token: &str, secret: &str) -> Result<Claims> {
    let data = jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    ).context("invalid or expired token")?;
    Ok(data.claims)
}

/// Check if claims indicate an admin user.
pub fn is_admin(claims: &Claims) -> bool {
    claims.role.as_deref() == Some("admin")
}

// ── Password hashing (argon2) ──

/// Hash a password using argon2id.
pub fn hash_password(password: &str) -> Result<String> {
    use argon2::password_hash::SaltString;
    use argon2::{Argon2, PasswordHasher};

    // Use argon2's own rand_core OsRng to avoid version conflicts
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("password hash failed: {e}"))?;
    Ok(hash.to_string())
}

/// Verify a password against a hash.
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    use argon2::{Argon2, PasswordHash, PasswordVerifier};

    let parsed = PasswordHash::new(hash)
        .map_err(|e| anyhow::anyhow!("invalid password hash: {e}"))?;
    Ok(Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

// ── TOTP ──

/// Generate a TOTP secret and otpauth URI for an email.
pub fn generate_totp_secret(email: &str) -> Result<(String, String)> {
    use totp_rs::{Algorithm, TOTP};

    // Generate 20 random bytes for the secret
    let mut secret_bytes = [0u8; 20];
    use rand::Rng;
    rand::rng().fill(&mut secret_bytes);

    let totp = TOTP::new(
        Algorithm::SHA1,
        6,    // digits
        1,    // skew
        30,   // step
        secret_bytes.to_vec(),
        Some("GACHA.AGENCY".to_string()),
        email.to_string(),
    ).map_err(|e| anyhow::anyhow!("totp creation error: {e}"))?;

    let uri = totp.get_url();
    let secret_base32 = totp.get_secret_base32();

    Ok((secret_base32, uri))
}

/// Verify a TOTP code against a base32 secret.
pub fn verify_totp(secret_base32: &str, code: &str) -> Result<bool> {
    use totp_rs::{Algorithm, TOTP};

    // Decode base32 secret to bytes
    let secret_bytes = base32_decode(secret_base32)
        .ok_or_else(|| anyhow::anyhow!("invalid base32 secret"))?;

    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("GACHA.AGENCY".to_string()),
        String::new(),
    ).map_err(|e| anyhow::anyhow!("totp creation error: {e}"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    Ok(totp.check(code, now))
}

/// Simple base32 decoder (RFC 4648).
fn base32_decode(input: &str) -> Option<Vec<u8>> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let input = input.as_bytes();
    let mut buffer = 0u64;
    let mut bits = 0;
    let mut result = Vec::new();

    for &c in input {
        if c == b'=' { break; }
        let val = alphabet.iter().position(|&a| a == c.to_ascii_uppercase())? as u64;
        buffer = (buffer << 5) | val;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            result.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }

    Some(result)
}

// ── Verification tokens ──

/// Generate a random 32-byte hex token for email verification or password reset.
pub fn generate_verification_token() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    hex::encode(bytes)
}
