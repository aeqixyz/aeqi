use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::AppState;
use crate::auth::AuthTenant;
use crate::types::*;

fn task_to_info(t: &system_tasks::Task) -> TaskInfo {
    TaskInfo {
        id: t.id.0.clone(),
        subject: t.subject.clone(),
        description: if t.description.is_empty() { None } else { Some(t.description.clone()) },
        status: t.status.to_string(),
        priority: t.priority.to_string(),
        assignee: t.assignee.clone(),
        mission_id: t.mission_id.clone(),
        labels: t.labels.clone(),
        created_at: t.created_at.to_rfc3339(),
        checkpoints: t.checkpoints.iter().map(|c| CheckpointInfo {
            timestamp: c.timestamp.to_rfc3339(),
            worker: c.worker.clone(),
            progress: c.progress.clone(),
            cost_usd: c.cost_usd,
        }).collect(),
    }
}

fn mission_to_info(m: &system_tasks::Mission, tasks: &[&system_tasks::Task]) -> MissionInfo {
    let (done, total) = system_tasks::Mission::check_progress(&m.id, tasks);
    MissionInfo {
        id: m.id.clone(),
        name: m.name.clone(),
        description: if m.description.is_empty() { None } else { Some(m.description.clone()) },
        status: m.status.to_string(),
        task_count: total as u32,
        completed_tasks: done as u32,
        labels: m.labels.clone(),
        created_at: m.created_at.to_rfc3339(),
    }
}

fn tenant_team(tenant: &system_tenants::Tenant) -> Option<TeamInfo> {
    let leader = tenant.leader()?;
    let mut agents = tenant.team();
    agents.retain(|a| a != &leader);
    Some(TeamInfo { leader, agents })
}

// GET /api/projects
pub async fn list_projects(
    AuthTenant(tenant): AuthTenant,
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Vec<ProjectInfo>>, (StatusCode, String)> {
    let team = tenant_team(&tenant);
    let summaries = tenant.registry.list_project_summaries().await;
    let infos: Vec<ProjectInfo> = summaries.into_iter().map(|s| ProjectInfo {
        name: s.name,
        prefix: s.prefix,
        team: team.clone(),
        open_tasks: s.open_tasks,
        total_tasks: s.total_tasks,
        active_missions: s.active_missions,
        total_missions: s.total_missions,
    }).collect();
    Ok(Json(infos))
}

// GET /api/projects/{name}
pub async fn get_project(
    AuthTenant(tenant): AuthTenant,
    State(_state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ProjectInfo>, (StatusCode, String)> {
    let project = tenant.registry.get_project(&name).await
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("project not found: {name}")))?;

    let board = project.tasks.lock().await;
    let all_tasks = board.all();
    let open_tasks = all_tasks.iter().filter(|t| !t.is_closed()).count() as u32;
    let total_tasks = all_tasks.len() as u32;

    let all_missions = board.missions(Some(&project.prefix));
    let active_missions = all_missions.iter().filter(|m| !m.is_closed()).count() as u32;
    let total_missions = all_missions.len() as u32;

    Ok(Json(ProjectInfo {
        name: project.name.clone(),
        prefix: project.prefix.clone(),
        team: tenant_team(&tenant),
        open_tasks,
        total_tasks,
        active_missions,
        total_missions,
    }))
}

// GET /api/projects/{name}/missions
pub async fn list_missions(
    AuthTenant(tenant): AuthTenant,
    Path(name): Path<String>,
) -> Result<Json<Vec<MissionInfo>>, (StatusCode, String)> {
    let project = tenant.registry.get_project(&name).await
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("project not found: {name}")))?;

    let board = project.tasks.lock().await;
    let all_tasks = board.all();
    let missions = board.missions(Some(&project.prefix));
    let infos: Vec<MissionInfo> = missions.iter().map(|m| mission_to_info(m, &all_tasks)).collect();
    Ok(Json(infos))
}

// GET /api/projects/{name}/missions/{id}
pub async fn get_mission(
    AuthTenant(tenant): AuthTenant,
    Path((name, mission_id)): Path<(String, String)>,
) -> Result<Json<MissionInfo>, (StatusCode, String)> {
    let project = tenant.registry.get_project(&name).await
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("project not found: {name}")))?;

    let board = project.tasks.lock().await;
    let mission = board.get_mission(&mission_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("mission not found: {mission_id}")))?;
    let all_tasks = board.all();
    Ok(Json(mission_to_info(mission, &all_tasks)))
}

// GET /api/projects/{name}/tasks
pub async fn list_tasks(
    AuthTenant(tenant): AuthTenant,
    Path(name): Path<String>,
    Query(params): Query<TaskQueryParams>,
) -> Result<Json<Vec<TaskInfo>>, (StatusCode, String)> {
    let project = tenant.registry.get_project(&name).await
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("project not found: {name}")))?;

    let board = project.tasks.lock().await;
    let mut tasks: Vec<&system_tasks::Task> = board.by_prefix(&project.prefix);

    if let Some(ref status) = params.status {
        tasks.retain(|t| t.status.to_string() == *status);
    }
    if let Some(ref assignee) = params.assignee {
        tasks.retain(|t| t.assignee.as_deref() == Some(assignee.as_str()));
    }

    let infos: Vec<TaskInfo> = tasks.iter().map(|t| task_to_info(t)).collect();
    Ok(Json(infos))
}

// GET /api/projects/{name}/tasks/{id}
pub async fn get_task(
    AuthTenant(tenant): AuthTenant,
    Path((name, task_id)): Path<(String, String)>,
) -> Result<Json<TaskInfo>, (StatusCode, String)> {
    let project = tenant.registry.get_project(&name).await
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("project not found: {name}")))?;

    let board = project.tasks.lock().await;
    let task = board.get(&task_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("task not found: {task_id}")))?;

    Ok(Json(task_to_info(task)))
}

// POST /api/projects/{name}/tasks
pub async fn create_task(
    AuthTenant(tenant): AuthTenant,
    Path(name): Path<String>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<TaskInfo>, (StatusCode, String)> {
    let project = tenant.registry.get_project(&name).await
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("project not found: {name}")))?;

    let mut board = project.tasks.lock().await;
    let mut task = board.create(&project.prefix, &req.subject)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Apply optional fields
    let task_id = task.id.0.clone();
    if req.description.is_some() || req.priority.is_some() || req.mission_id.is_some() {
        task = board.update(&task_id, |t| {
            if let Some(ref desc) = req.description {
                t.description = desc.clone();
            }
            if let Some(ref priority) = req.priority {
                t.priority = match priority.as_str() {
                    "low" => system_tasks::Priority::Low,
                    "high" => system_tasks::Priority::High,
                    "critical" => system_tasks::Priority::Critical,
                    _ => system_tasks::Priority::Normal,
                };
            }
            if let Some(ref mid) = req.mission_id {
                t.mission_id = Some(mid.clone());
            }
        }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    Ok(Json(task_to_info(&task)))
}

// PATCH /api/projects/{name}/tasks/{id}
pub async fn update_task(
    AuthTenant(tenant): AuthTenant,
    Path((name, task_id)): Path<(String, String)>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<Json<TaskInfo>, (StatusCode, String)> {
    let project = tenant.registry.get_project(&name).await
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("project not found: {name}")))?;

    let mut board = project.tasks.lock().await;

    // Verify task exists
    board.get(&task_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("task not found: {task_id}")))?;

    let task = board.update(&task_id, |t| {
        if let Some(ref status) = req.status {
            t.status = match status.as_str() {
                "pending" => system_tasks::TaskStatus::Pending,
                "in_progress" => system_tasks::TaskStatus::InProgress,
                "done" => system_tasks::TaskStatus::Done,
                "blocked" => system_tasks::TaskStatus::Blocked,
                "cancelled" => system_tasks::TaskStatus::Cancelled,
                _ => t.status,
            };
        }
        if let Some(ref assignee) = req.assignee {
            t.assignee = if assignee.is_empty() { None } else { Some(assignee.clone()) };
        }
        if let Some(ref desc) = req.description {
            t.description = desc.clone();
        }
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(task_to_info(&task)))
}

// GET /api/active-project
pub async fn get_active_project(
    AuthTenant(tenant): AuthTenant,
) -> Result<Json<ActiveProjectResponse>, (StatusCode, String)> {
    let active = tenant.active_project().await;
    Ok(Json(ActiveProjectResponse { active_project: active }))
}

// PUT /api/active-project
pub async fn set_active_project(
    AuthTenant(tenant): AuthTenant,
    Json(req): Json<SetActiveProjectRequest>,
) -> Result<Json<ActiveProjectResponse>, (StatusCode, String)> {
    // Validate project exists if setting one
    if let Some(ref name) = req.name {
        tenant.registry.get_project(name).await
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("project not found: {name}")))?;
    }
    tenant.set_active_project(req.name.clone()).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(ActiveProjectResponse { active_project: req.name }))
}

// DELETE /api/projects/{name}
pub async fn delete_project(
    AuthTenant(tenant): AuthTenant,
    Path(name): Path<String>,
) -> Result<Json<DeleteProjectResponse>, (StatusCode, String)> {
    // Check project exists
    tenant.registry.get_project(&name).await
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("project not found: {name}")))?;

    // Remove from registry
    tenant.registry.remove_project(&name).await;

    // Clear active_project if it was this one
    if tenant.active_project().await.as_deref() == Some(&name) {
        let _ = tenant.set_active_project(None).await;
    }

    // Remove project directory from disk — but only if it's under data_dir (tenant-owned).
    // If projects_source is set and the project lives there, skip physical deletion.
    let local_project_dir = tenant.data_dir.join("projects").join(&name);
    if local_project_dir.is_dir() {
        std::fs::remove_dir_all(&local_project_dir)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("failed to remove project dir: {e}")))?;
    }

    Ok(Json(DeleteProjectResponse { deleted: true }))
}

// POST /api/projects/{name}/missions
pub async fn create_mission(
    AuthTenant(tenant): AuthTenant,
    Path(name): Path<String>,
    Json(req): Json<CreateMissionRequest>,
) -> Result<Json<MissionInfo>, (StatusCode, String)> {
    let project = tenant.registry.get_project(&name).await
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("project not found: {name}")))?;

    let mut board = project.tasks.lock().await;
    let mut mission = board.create_mission(&project.prefix, &req.name)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(ref desc) = req.description {
        mission = board.update_mission(&mission.id, |m| {
            m.description = desc.clone();
        }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let all_tasks = board.all();
    Ok(Json(mission_to_info(&mission, &all_tasks)))
}
