use anyhow::Result;
use tracing::{info, warn};

/// Transactional email service via Resend API.
#[derive(Clone)]
pub struct EmailService {
    api_key: String,
    from: String,
    base_url: String,
    client: reqwest::Client,
}

impl EmailService {
    pub fn new(api_key: &str, from: Option<&str>, base_url: Option<&str>) -> Self {
        Self {
            api_key: api_key.to_string(),
            from: from.unwrap_or("aeqi <hello@aeqi.ai>").to_string(),
            base_url: base_url.unwrap_or("https://app.aeqi.ai").to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn send(&self, to: &str, subject: &str, html: &str) -> Result<()> {
        let resp = self
            .client
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({
                "from": self.from,
                "to": [to],
                "subject": subject,
                "html": html,
            }))
            .send()
            .await?;

        if resp.status().is_success() {
            info!(to, subject, "email sent");
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!(to, subject, %status, body, "email send failed");
            anyhow::bail!("email send failed: {status} {body}")
        }
    }

    pub async fn send_verification(&self, to: &str, name: &str, code: &str) {
        let html = verification_email(code, name);
        if let Err(e) = self.send(to, "Verify your email", &html).await {
            warn!(error = %e, "failed to send verification email");
        }
    }

    pub async fn send_welcome(&self, to: &str, name: &str) {
        let html = welcome_email(name, &self.base_url);
        if let Err(e) = self.send(to, "Welcome to aeqi", &html).await {
            warn!(error = %e, "failed to send welcome email");
        }
    }

    pub async fn send_login_notification(
        &self,
        to: &str,
        name: &str,
        device: &str,
        ip: &str,
        time: &str,
    ) {
        let html = login_notification_email(name, device, ip, time, &self.base_url);
        if let Err(e) = self.send(to, "New login to aeqi", &html).await {
            warn!(error = %e, "failed to send login notification");
        }
    }
}

// ── Email Templates ─────────────────────────────────────

fn email_wrapper(content: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
  body {{ margin: 0; padding: 0; background: #ffffff; font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; color: rgba(0,0,0,0.85); }}
  .container {{ max-width: 480px; margin: 0 auto; padding: 48px 24px; }}
  .logo {{ font-size: 32px; font-weight: 700; color: rgba(0,0,0,0.4); letter-spacing: -0.03em; text-align: center; margin-bottom: 32px; }}
  .heading {{ font-size: 20px; font-weight: 600; letter-spacing: -0.02em; text-align: center; margin: 0 0 8px; }}
  .subtext {{ font-size: 14px; color: rgba(0,0,0,0.4); text-align: center; margin: 0 0 32px; line-height: 1.5; }}
  .code {{ font-size: 32px; font-weight: 700; letter-spacing: 0.15em; text-align: center; padding: 24px; background: rgba(0,0,0,0.03); border-radius: 12px; margin: 0 0 32px; font-family: 'JetBrains Mono', monospace; }}
  .btn {{ display: inline-block; padding: 12px 32px; background: #000000; color: #ffffff !important; border-radius: 999px; font-size: 14px; font-weight: 500; text-decoration: none; }}
  .btn-wrap {{ text-align: center; margin: 24px 0; }}
  .detail {{ font-size: 13px; color: rgba(0,0,0,0.5); line-height: 1.6; }}
  .detail-row {{ padding: 8px 0; border-bottom: 1px solid rgba(0,0,0,0.06); }}
  .detail-label {{ color: rgba(0,0,0,0.3); }}
  .footer {{ margin-top: 48px; text-align: center; font-size: 11px; color: rgba(0,0,0,0.2); line-height: 1.6; }}
  .footer a {{ color: rgba(0,0,0,0.3); }}
</style>
</head>
<body>
<div class="container">
  <div class="logo">æqi</div>
  {content}
  <div class="footer">
    <p>aeqi — agent orchestration runtime</p>
    <p>If you didn't request this, you can ignore this email.</p>
  </div>
</div>
</body>
</html>"#
    )
}

fn verification_email(code: &str, name: &str) -> String {
    let greeting = if name.is_empty() {
        "Verify your email".to_string()
    } else {
        format!("Hey {name}")
    };
    email_wrapper(&format!(
        r#"<h1 class="heading">{greeting}</h1>
<p class="subtext">Enter this code to verify your email and get started.</p>
<div class="code">{code}</div>
<p class="subtext">This code expires in 10 minutes.</p>"#
    ))
}

fn welcome_email(name: &str, base_url: &str) -> String {
    let greeting = if name.is_empty() {
        "Welcome to aeqi".to_string()
    } else {
        format!("Welcome, {name}")
    };
    email_wrapper(&format!(
        r#"<h1 class="heading">{greeting}</h1>
<p class="subtext">Your account is verified. You're ready to build with autonomous agents.</p>
<div class="btn-wrap">
  <a href="{base_url}" class="btn">Open aeqi</a>
</div>
<p class="subtext" style="margin-top:32px">Here's what to do next:</p>
<div class="detail">
  <div class="detail-row"><strong>1.</strong> Create your first company</div>
  <div class="detail-row"><strong>2.</strong> Hire your first agent</div>
  <div class="detail-row"><strong>3.</strong> Assign work and watch it run</div>
</div>"#
    ))
}

fn login_notification_email(
    _name: &str,
    device: &str,
    ip: &str,
    time: &str,
    base_url: &str,
) -> String {
    let greeting = "New login to aeqi".to_string();
    email_wrapper(&format!(
        r#"<h1 class="heading">{greeting}</h1>
<p class="subtext">We noticed a login to your aeqi account from a new device.</p>
<div class="detail">
  <div class="detail-row"><span class="detail-label">Device</span><br>{device}</div>
  <div class="detail-row"><span class="detail-label">IP Address</span><br>{ip}</div>
  <div class="detail-row"><span class="detail-label">Time</span><br>{time}</div>
</div>
<div class="btn-wrap" style="margin-top:32px">
  <a href="{base_url}/settings" class="btn">Review sessions</a>
</div>
<p class="subtext" style="margin-top:24px">If you didn't authorize this, change your password immediately.</p>"#
    ))
}
