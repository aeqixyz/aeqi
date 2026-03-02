use anyhow::{Context, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::Rng;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Encrypted secret store using ChaCha20-Poly1305.
pub struct SecretStore {
    path: PathBuf,
    key: [u8; 32],
}

impl SecretStore {
    /// Initialize or open a secret store.
    pub fn open(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path)
            .with_context(|| format!("failed to create secret store: {}", path.display()))?;

        let key_path = path.join(".key");
        let key = if key_path.exists() {
            let encoded = std::fs::read_to_string(&key_path)
                .context("failed to read secret store key")?;
            let decoded = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                encoded.trim(),
            )
            .context("failed to decode secret store key")?;
            let mut key = [0u8; 32];
            key.copy_from_slice(&decoded);
            key
        } else {
            let mut key = [0u8; 32];
            rand::rng().fill(&mut key);
            let encoded = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                key,
            );
            std::fs::write(&key_path, &encoded)
                .context("failed to write secret store key")?;

            // Restrict permissions on the key file.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
            }

            key
        };

        Ok(Self {
            path: path.to_path_buf(),
            key,
        })
    }

    /// Store an encrypted secret.
    pub fn set(&self, name: &str, value: &str) -> Result<()> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|e| anyhow::anyhow!("cipher init failed: {e}"))?;

        let mut nonce_bytes = [0u8; 12];
        rand::rng().fill(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, value.as_bytes())
            .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;

        // Store as: nonce (12 bytes) || ciphertext
        let mut data = Vec::with_capacity(12 + ciphertext.len());
        data.extend_from_slice(&nonce_bytes);
        data.extend_from_slice(&ciphertext);

        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &data,
        );

        let file_path = self.path.join(format!("{name}.enc"));
        std::fs::write(&file_path, &encoded)
            .with_context(|| format!("failed to write secret: {name}"))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }

    /// Retrieve a decrypted secret.
    pub fn get(&self, name: &str) -> Result<String> {
        let file_path = self.path.join(format!("{name}.enc"));
        let encoded = std::fs::read_to_string(&file_path)
            .with_context(|| format!("secret not found: {name}"))?;

        let data = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            encoded.trim(),
        )
        .context("failed to decode secret")?;

        if data.len() < 12 {
            anyhow::bail!("corrupt secret: {name}");
        }

        let nonce = Nonce::from_slice(&data[..12]);
        let ciphertext = &data[12..];

        let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|e| anyhow::anyhow!("cipher init failed: {e}"))?;

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))?;

        String::from_utf8(plaintext).context("secret is not valid UTF-8")
    }

    /// List all secret names.
    pub fn list(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".enc") {
                names.push(name.trim_end_matches(".enc").to_string());
            }
        }
        names.sort();
        Ok(names)
    }

    /// Delete a secret.
    pub fn delete(&self, name: &str) -> Result<()> {
        let file_path = self.path.join(format!("{name}.enc"));
        if file_path.exists() {
            std::fs::remove_file(&file_path)
                .with_context(|| format!("failed to delete secret: {name}"))?;
        }
        Ok(())
    }

    /// Load all secrets into a HashMap (for env injection).
    pub fn load_all(&self) -> Result<HashMap<String, String>> {
        let mut secrets = HashMap::new();
        for name in self.list()? {
            if let Ok(value) = self.get(&name) {
                secrets.insert(name, value);
            }
        }
        Ok(secrets)
    }
}
