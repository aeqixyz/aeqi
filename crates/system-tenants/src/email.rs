use anyhow::{Context, Result};
use serde::Serialize;

use crate::config::EmailConfig;

/// Email service using Resend HTTP API.
pub struct EmailService {
    config: EmailConfig,
    client: reqwest::Client,
}

#[derive(Serialize)]
struct ResendPayload {
    from: String,
    to: Vec<String>,
    subject: String,
    html: String,
}

impl EmailService {
    pub fn new(config: EmailConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    /// Send an email verification link.
    pub async fn send_verification(&self, to: &str, token: &str, base_url: &str) -> Result<()> {
        let link = format!("{}/auth/verify-email?token={}", base_url, token);
        let subject = "Verify your GACHA.AGENCY email";
        let html = format!(
            r#"<div style="font-family:monospace;max-width:500px;margin:0 auto;padding:40px 20px">
            <h1 style="font-size:18px;letter-spacing:4px;color:#111">GACHA.AGENCY</h1>
            <p style="font-size:13px;color:#666;margin-top:20px">Welcome, Daemon.</p>
            <p style="font-size:13px;color:#666">Verify your comms channel to begin your first summoning:</p>
            <a href="{link}" style="display:inline-block;margin:24px 0;padding:12px 32px;background:#111;color:#fff;text-decoration:none;font-size:11px;letter-spacing:3px">VERIFY EMAIL</a>
            <p style="font-size:11px;color:#aaa;margin-top:20px">This link expires in 24 hours. If you didn't create an account, ignore this transmission.</p>
            </div>"#,
        );
        self.send(to, subject, &html).await
    }

    /// Send a password reset link.
    pub async fn send_password_reset(&self, to: &str, token: &str, base_url: &str) -> Result<()> {
        let link = format!("{}/auth/reset-password?token={}", base_url, token);
        let subject = "Reset your GACHA.AGENCY passphrase";
        let html = format!(
            r#"<div style="font-family:monospace;max-width:500px;margin:0 auto;padding:40px 20px">
            <h1 style="font-size:18px;letter-spacing:4px;color:#111">GACHA.AGENCY</h1>
            <p style="font-size:13px;color:#666;margin-top:20px">A passphrase reset was requested for your account.</p>
            <a href="{link}" style="display:inline-block;margin:24px 0;padding:12px 32px;background:#111;color:#fff;text-decoration:none;font-size:11px;letter-spacing:3px">RESET PASSPHRASE</a>
            <p style="font-size:11px;color:#aaa;margin-top:20px">This link expires in 1 hour. If you didn't request this, ignore this transmission.</p>
            </div>"#,
        );
        self.send(to, subject, &html).await
    }

    async fn send(&self, to: &str, subject: &str, html: &str) -> Result<()> {
        let from = format!("{} <{}>", self.config.from_name, self.config.from_address);
        let payload = ResendPayload {
            from,
            to: vec![to.to_string()],
            subject: subject.to_string(),
            html: html.to_string(),
        };

        let resp = self.client
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&payload)
            .send()
            .await
            .context("failed to call Resend API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Resend API error {}: {}", status, body);
        }

        Ok(())
    }
}
