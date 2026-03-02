use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use system_core::traits::{ToolResult, ToolSpec};
use tracing::debug;

const BASE_URL: &str = "https://api.porkbun.com/api/json/v3";

pub struct PorkbunTool {
    api_key: String,
    secret_key: String,
    client: Client,
}

impl PorkbunTool {
    pub fn new(api_key: String, secret_key: String) -> Self {
        Self {
            api_key,
            secret_key,
            client: Client::new(),
        }
    }

    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("PORKBUN_API_KEY").ok()?;
        let secret_key = std::env::var("PORKBUN_SECRET_KEY").ok()?;
        Some(Self::new(api_key, secret_key))
    }

    fn auth_body(&self) -> Value {
        json!({
            "apikey": self.api_key,
            "secretapikey": self.secret_key,
        })
    }

    fn merge_body(&self, extra: Value) -> Value {
        let mut body = self.auth_body();
        if let (Some(obj), Some(ext)) = (body.as_object_mut(), extra.as_object()) {
            for (k, v) in ext {
                obj.insert(k.clone(), v.clone());
            }
        }
        body
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value> {
        let url = format!("{BASE_URL}{path}");
        debug!(url = %url, "porkbun request");

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .json::<Value>()
            .await?;

        Ok(resp)
    }

    fn format_response(resp: Value) -> ToolResult {
        let status = resp.get("status").and_then(|s| s.as_str()).unwrap_or("UNKNOWN");
        if status == "SUCCESS" {
            ToolResult::success(serde_json::to_string_pretty(&resp).unwrap_or_default())
        } else {
            let msg = resp.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error");
            ToolResult::error(format!("Porkbun error: {msg}\n{}", serde_json::to_string_pretty(&resp).unwrap_or_default()))
        }
    }
}

#[async_trait]
impl system_core::traits::Tool for PorkbunTool {
    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'action' argument"))?;

        let result = match action {
            "ping" => {
                let body = self.auth_body();
                self.post("/ping", body).await?
            }

            "check_availability" => {
                let domain = args.get("domain").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'domain'"))?;
                let body = self.merge_body(json!({ "domain": domain }));
                self.post("/domain/checkAvailability", body).await?
            }

            "list_domains" => {
                let mut extra = json!({});
                if let Some(start) = args.get("start").and_then(|v| v.as_u64()) {
                    extra["start"] = json!(start);
                }
                if let Some(count) = args.get("count").and_then(|v| v.as_u64()) {
                    extra["includeLabels"] = json!("yes");
                    let _ = count;
                }
                let body = self.merge_body(extra);
                self.post("/domain/list", body).await?
            }

            "buy_domain" => {
                let domain = args.get("domain").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'domain'"))?;
                let years = args.get("years").and_then(|v| v.as_u64()).unwrap_or(1);
                let body = self.merge_body(json!({
                    "domain": domain,
                    "years": years,
                }));
                self.post("/domain/create", body).await?
            }

            "list_dns" => {
                let domain = args.get("domain").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'domain'"))?;
                let body = self.auth_body();
                self.post(&format!("/dns/retrieve/{domain}"), body).await?
            }

            "create_dns" => {
                let domain = args.get("domain").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'domain'"))?;
                let record_type = args.get("type").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'type' (A, AAAA, MX, CNAME, TXT, etc.)"))?;
                let content = args.get("content").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
                let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let ttl = args.get("ttl").and_then(|v| v.as_str()).unwrap_or("600");
                let prio = args.get("prio").and_then(|v| v.as_str()).unwrap_or("0");
                let body = self.merge_body(json!({
                    "name": name,
                    "type": record_type,
                    "content": content,
                    "ttl": ttl,
                    "prio": prio,
                }));
                self.post(&format!("/dns/create/{domain}"), body).await?
            }

            "edit_dns" => {
                let domain = args.get("domain").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'domain'"))?;
                let id = args.get("id").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;
                let record_type = args.get("type").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'type'"))?;
                let content = args.get("content").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
                let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let ttl = args.get("ttl").and_then(|v| v.as_str()).unwrap_or("600");
                let prio = args.get("prio").and_then(|v| v.as_str()).unwrap_or("0");
                let body = self.merge_body(json!({
                    "name": name,
                    "type": record_type,
                    "content": content,
                    "ttl": ttl,
                    "prio": prio,
                }));
                self.post(&format!("/dns/edit/{domain}/{id}"), body).await?
            }

            "delete_dns" => {
                let domain = args.get("domain").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'domain'"))?;
                let id = args.get("id").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;
                let body = self.auth_body();
                self.post(&format!("/dns/delete/{domain}/{id}"), body).await?
            }

            "list_email_forwards" => {
                let domain = args.get("domain").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'domain'"))?;
                let body = self.auth_body();
                self.post(&format!("/domain/getEmailForwarding/{domain}"), body).await?
            }

            "create_email_forward" => {
                let domain = args.get("domain").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'domain'"))?;
                let alias = args.get("alias").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'alias' (the local part before @)"))?;
                let destination = args.get("destination").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'destination' (target email address)"))?;
                let body = self.merge_body(json!({
                    "alias": alias,
                    "destination": destination,
                }));
                self.post(&format!("/domain/addEmailForwarding/{domain}"), body).await?
            }

            "delete_email_forward" => {
                let domain = args.get("domain").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'domain'"))?;
                let id = args.get("id").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;
                let body = self.auth_body();
                self.post(&format!("/domain/deleteEmailForwarding/{domain}/{id}"), body).await?
            }

            other => {
                return Ok(ToolResult::error(format!("unknown action: {other}")));
            }
        };

        Ok(Self::format_response(result))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "porkbun".to_string(),
            description: "Porkbun domain registrar API. Check availability, buy domains, manage DNS records and email forwarding.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": [
                            "ping",
                            "check_availability",
                            "list_domains",
                            "buy_domain",
                            "list_dns",
                            "create_dns",
                            "edit_dns",
                            "delete_dns",
                            "list_email_forwards",
                            "create_email_forward",
                            "delete_email_forward"
                        ],
                        "description": "Action to perform"
                    },
                    "domain": {
                        "type": "string",
                        "description": "Domain name (e.g. example.com). Required for all actions except ping and list_domains."
                    },
                    "years": {
                        "type": "integer",
                        "description": "Registration years for buy_domain (default: 1)"
                    },
                    "type": {
                        "type": "string",
                        "description": "DNS record type for create_dns/edit_dns (A, AAAA, MX, CNAME, TXT, NS, SRV, etc.)"
                    },
                    "content": {
                        "type": "string",
                        "description": "DNS record value for create_dns/edit_dns"
                    },
                    "name": {
                        "type": "string",
                        "description": "DNS record subdomain for create_dns/edit_dns. Empty string for apex record."
                    },
                    "ttl": {
                        "type": "string",
                        "description": "TTL in seconds for DNS records (default: 600)"
                    },
                    "prio": {
                        "type": "string",
                        "description": "Priority for MX/SRV DNS records (default: 0)"
                    },
                    "id": {
                        "type": "string",
                        "description": "Record ID for edit_dns, delete_dns, delete_email_forward"
                    },
                    "alias": {
                        "type": "string",
                        "description": "Email alias (local part before @) for create_email_forward"
                    },
                    "destination": {
                        "type": "string",
                        "description": "Destination email address for create_email_forward"
                    },
                    "start": {
                        "type": "integer",
                        "description": "Pagination offset for list_domains (default: 0)"
                    }
                },
                "required": ["action"]
            }),
        }
    }

    fn name(&self) -> &str {
        "porkbun"
    }
}
