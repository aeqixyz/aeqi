use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// --- Auth (legacy anonymous) ---
#[derive(Deserialize)]
pub struct RegisterRequest {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Serialize)]
pub struct RegisterResponse {
    pub token: String,
    pub tenant_id: String,
    pub companion: CompanionInfo,
}

#[derive(Serialize)]
pub struct RefreshResponse {
    pub token: String,
}

// --- Auth (email+password) ---
#[derive(Deserialize)]
pub struct EmailRegisterRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Serialize)]
pub struct EmailRegisterResponse {
    pub token: String,
    pub tenant_id: String,
    pub companion: CompanionInfo,
    pub requires_email_verification: bool,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub totp_code: Option<String>,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: Option<String>,
    pub requires_totp: bool,
    pub tenant_id: Option<String>,
}

#[derive(Deserialize)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[derive(Serialize)]
pub struct VerifyEmailResponse {
    pub verified: bool,
}

#[derive(Serialize)]
pub struct TotpSetupResponse {
    pub secret: String,
    pub uri: String,
}

#[derive(Deserialize)]
pub struct TotpVerifyRequest {
    pub code: String,
}

#[derive(Serialize)]
pub struct TotpVerifyResponse {
    pub enabled: bool,
}

#[derive(Deserialize)]
pub struct TotpDisableRequest {
    pub code: String,
}

#[derive(Deserialize)]
pub struct PasswordResetRequest {
    pub email: String,
}

#[derive(Deserialize)]
pub struct PasswordResetConfirm {
    pub token: String,
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current: String,
    pub new_password: String,
}

#[derive(Serialize)]
pub struct SuccessResponse {
    pub success: bool,
}

// --- Companions ---
#[derive(Serialize)]
pub struct CompanionInfo {
    pub name: String,
    pub full_name: String,
    pub rarity: String,
    pub archetype: String,
    pub aesthetic: String,
    pub region: String,
    pub bond_level: u32,
    pub interaction_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dere_type: Option<String>,
    pub is_familiar: bool,
    pub anime_inspirations: Vec<AnimeInspirationInfo>,
    pub persona_status: String,
    pub portrait_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
}

#[derive(Serialize)]
pub struct AnimeInspirationInfo {
    pub name: String,
    pub genre: String,
}

#[derive(Deserialize)]
pub struct SetFamiliarRequest {
    pub name: String,
}

// --- Gacha ---
#[derive(Serialize)]
pub struct PullResponse {
    pub companion: CompanionInfo,
    pub is_new: bool,
    pub pity_count: u32,
}

#[derive(Serialize)]
pub struct Pull10Response {
    pub results: Vec<PullResponse>,
}

#[derive(Deserialize)]
pub struct FuseRequest {
    pub names: Vec<String>,
}

#[derive(Serialize)]
pub struct FuseResponse {
    pub companion: CompanionInfo,
}

// --- Chat ---
#[derive(Serialize)]
pub struct ChatHistoryEntry {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize { 50 }

// --- User ---
#[derive(Serialize)]
pub struct ProfileResponse {
    pub display_name: String,
    pub tier: String,
    pub companions_count: usize,
    pub created_at: DateTime<Utc>,
    pub email: Option<String>,
    pub totp_enabled: bool,
}

#[derive(Serialize)]
pub struct UsageResponse {
    pub cost_today_usd: f64,
    pub storage_mb: f64,
    pub companions_count: usize,
    pub tier_limit_companions: u32,
    pub tier_limit_cost_usd: f64,
}

// --- Economy ---
#[derive(Serialize)]
pub struct EconomyResponse {
    pub summons: i64,
    pub summons_max: u32,
    pub mana: i64,
    pub mana_max: u32,
}

// --- WebSocket ---
#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum WsClientMessage {
    #[serde(rename = "message")]
    Message { content: String },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum WsServerMessage {
    #[serde(rename = "typing")]
    Typing { companion: String },
    #[serde(rename = "message")]
    Message {
        companion: String,
        content: String,
        timestamp: i64,
    },
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "party")]
    Party { leader: String, squad: Vec<String> },
    #[serde(rename = "advisor_message")]
    AdvisorMessage {
        companion: String,
        content: String,
        timestamp: i64,
    },
}

// --- Party / Squad ---
#[derive(Serialize)]
pub struct PartyResponse {
    pub leader: Option<CompanionInfo>,
    pub squad: Vec<CompanionInfo>,
    pub max_size: u32,
}

#[derive(Deserialize)]
pub struct SetSquadRequest {
    pub members: Vec<String>,
}

#[derive(Deserialize)]
pub struct SetLeaderRequest {
    pub name: String,
}

// --- Admin ---
#[derive(Serialize)]
pub struct AdminStatsResponse {
    pub active_tenants: usize,
    pub global_cost_today_usd: f64,
}

// --- Stripe ---
#[derive(Deserialize)]
pub struct CheckoutRequest {
    pub tier: String,
}

#[derive(Serialize)]
pub struct CheckoutResponse {
    pub url: String,
}

#[derive(Serialize)]
pub struct PortalResponse {
    pub url: String,
}

// --- Relationships ---
#[derive(Serialize)]
pub struct RelationshipInfo {
    pub companion_a: String,
    pub companion_b: String,
    pub respect: f32,
    pub affinity: f32,
    pub trust: f32,
    pub rivalry: f32,
    pub synergy: f32,
    pub label: String,
    pub compatibility: f32,
}

// --- Projects / Missions / Tasks ---
#[derive(Serialize)]
pub struct ProjectInfo {
    pub name: String,
    pub prefix: String,
    pub team: Option<TeamInfo>,
    pub open_tasks: u32,
    pub total_tasks: u32,
    pub active_missions: u32,
    pub total_missions: u32,
}

#[derive(Serialize, Clone)]
pub struct TeamInfo {
    pub leader: String,
    pub agents: Vec<String>,
}

#[derive(Serialize)]
pub struct MissionInfo {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    pub task_count: u32,
    pub completed_tasks: u32,
    pub labels: Vec<String>,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct TaskInfo {
    pub id: String,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mission_id: Option<String>,
    pub labels: Vec<String>,
    pub created_at: String,
    pub checkpoints: Vec<CheckpointInfo>,
}

#[derive(Serialize)]
pub struct CheckpointInfo {
    pub timestamp: String,
    pub worker: String,
    pub progress: String,
    pub cost_usd: f64,
}

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub subject: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub mission_id: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateTaskRequest {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateMissionRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct TaskQueryParams {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
}

#[derive(Serialize)]
pub struct ActiveProjectResponse {
    pub active_project: Option<String>,
}

#[derive(Deserialize)]
pub struct SetActiveProjectRequest {
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct DeleteProjectResponse {
    pub deleted: bool,
}

// --- Helpers ---
impl CompanionInfo {
    pub fn from_companion(c: &system_companions::Companion) -> Self {
        Self {
            name: c.name.clone(),
            full_name: c.full_name(),
            rarity: c.rarity.to_string(),
            archetype: format!("{:?}", c.archetype),
            aesthetic: c.aesthetic.to_string(),
            region: c.region.to_string(),
            bond_level: c.bond_level,
            interaction_count: c.bond_xp,
            dere_type: Some(format!("{:?}", c.dere_type)),
            is_familiar: c.is_familiar,
            anime_inspirations: c.anime_inspirations.iter().map(|a| AnimeInspirationInfo {
                name: a.name.clone(),
                genre: format!("{:?}", a.genre),
            }).collect(),
            persona_status: format!("{:?}", c.persona_status),
            portrait_status: format!("{:?}", c.portrait_status),
            title: c.title.clone(),
            last_name: c.last_name.clone(),
        }
    }
}
