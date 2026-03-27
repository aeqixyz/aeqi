import { useEffect, useState } from "react";
import { useParams, Link } from "react-router-dom";
import Header from "@/components/Header";
import StatusBadge from "@/components/StatusBadge";
import MissionCard from "@/components/MissionCard";
import AuditEntryComponent from "@/components/AuditEntry";
import { PRIORITY_COLORS } from "@/lib/constants";
import { api } from "@/lib/api";
import { runtimeLabel, summarizeTaskRuntime } from "@/lib/runtime";

export default function ProjectDetailPage() {
  const { name } = useParams<{ name: string }>();
  const [project, setProject] = useState<any>(null);
  const [tasks, setTasks] = useState<any[]>([]);
  const [missions, setMissions] = useState<any[]>([]);
  const [audit, setAudit] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState<"tasks" | "missions" | "audit">("tasks");

  useEffect(() => {
    if (!name) return;
    setLoading(true);

    Promise.all([
      api.getProjects().then((d) => {
        const p = (d.projects || []).find((p: any) => p.name === name);
        setProject(p || null);
      }),
      api.getTasks({ project: name }).then((d) => setTasks(d.tasks || [])),
      api.getMissions({ project: name }).then((d) => setMissions(d.missions || [])),
      api.getAudit({ project: name, last: 30 }).then((d) => setAudit(d.events || [])),
    ])
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [name]);

  if (loading) return <div className="loading">Loading project...</div>;
  if (!project) return <div className="loading">Project not found</div>;

  const pendingTasks = tasks.filter((t) => t.status === "pending");
  const activeTasks = tasks.filter((t) => t.status === "in_progress");
  const doneTasks = tasks.filter((t) => t.status === "done");
  const total = tasks.length;
  const donePct = total > 0 ? (doneTasks.length / total) * 100 : 0;

  return (
    <>
      <Header
        title={project.name}
        breadcrumbs={[
          { label: "Projects", href: "/projects" },
          { label: project.name },
        ]}
      />

      {/* Hero Stats */}
      <div className="hero-stats" style={{ marginBottom: "var(--space-6)" }}>
        <div className="hero-stat">
          <div className="hero-stat-value">{total}</div>
          <div className="hero-stat-label">Total Tasks</div>
        </div>
        <div className="hero-stat-divider" />
        <div className="hero-stat">
          <div className="hero-stat-value" style={{ color: "var(--text-muted)" }}>{pendingTasks.length}</div>
          <div className="hero-stat-label">Pending</div>
        </div>
        <div className="hero-stat-divider" />
        <div className="hero-stat">
          <div className="hero-stat-value" style={{ color: "var(--info)" }}>{activeTasks.length}</div>
          <div className="hero-stat-label">In Progress</div>
        </div>
        <div className="hero-stat-divider" />
        <div className="hero-stat">
          <div className="hero-stat-value" style={{ color: "var(--success)" }}>{doneTasks.length}</div>
          <div className="hero-stat-label">Done</div>
        </div>
        <div className="hero-stat-divider" />
        <div className="hero-stat">
          <div className="hero-stat-value">{missions.length}</div>
          <div className="hero-stat-label">Missions</div>
        </div>
      </div>

      {/* Project Info */}
      <div className="detail-grid">
        <div className="detail-sidebar">
          {/* Info Panel */}
          <div className="detail-panel">
            <div className="detail-panel-title">Project Info</div>
            <div className="detail-field">
              <div className="detail-field-label">Prefix</div>
              <div className="detail-field-value"><code>{project.prefix}</code></div>
            </div>
            {project.team && (
              <>
                <div className="detail-field">
                  <div className="detail-field-label">Team Leader</div>
                  <div className="detail-field-value">{project.team.leader}</div>
                </div>
                <div className="detail-field">
                  <div className="detail-field-label">Team</div>
                  <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--space-1)" }}>
                    {(project.team.agents || []).map((a: string) => (
                      <span key={a} className="expertise-tag">{a}</span>
                    ))}
                  </div>
                </div>
              </>
            )}
            <div className="detail-field">
              <div className="detail-field-label">Progress</div>
              <div className="progress-bar-bg" style={{ marginTop: "var(--space-1)" }}>
                <div className="progress-bar-fill" style={{ width: `${donePct}%` }} />
              </div>
              <div style={{ fontSize: "var(--font-size-xs)", color: "var(--text-muted)", marginTop: "4px" }}>
                {donePct.toFixed(0)}% complete
              </div>
            </div>
          </div>
        </div>

        {/* Main Content */}
        <div className="detail-main">
          {/* Tabs */}
          <div style={{ display: "flex", gap: "var(--space-1)", marginBottom: "var(--space-4)" }}>
            {(["tasks", "missions", "audit"] as const).map((t) => (
              <button
                key={t}
                className={`btn ${tab === t ? "btn-primary" : ""}`}
                onClick={() => setTab(t)}
              >
                {t === "tasks" ? `Tasks (${tasks.length})` : t === "missions" ? `Missions (${missions.length})` : `Audit (${audit.length})`}
              </button>
            ))}
          </div>

          {/* Tasks Tab */}
          {tab === "tasks" && (
                <div className="task-table">
                  {tasks.length === 0 ? (
                    <div className="dash-empty">No tasks in this project</div>
                  ) : (
                    tasks.slice(0, 50).map((task: any) => {
                      const label = runtimeLabel(task.runtime);
                      const detail = summarizeTaskRuntime(task.runtime, task.closed_reason);

                      return (
                        <div key={task.id} className="task-row">
                          <span
                            className="task-priority-bar"
                            style={{ backgroundColor: PRIORITY_COLORS[task.priority] || "var(--text-primary)" }}
                          />
                          <code className="task-id">{task.id}</code>
                          <div style={{ minWidth: 0, display: "flex", flexDirection: "column", gap: "2px" }}>
                            <span className="task-subject">{task.subject}</span>
                            {(label || detail) && (
                              <span
                                style={{
                                  fontSize: "var(--font-size-xs)",
                                  color: "var(--text-muted)",
                                  overflow: "hidden",
                                  textOverflow: "ellipsis",
                                  whiteSpace: "nowrap",
                                }}
                              >
                                {[label, detail].filter(Boolean).join(" • ")}
                              </span>
                            )}
                          </div>
                          <div className="task-meta">
                            <StatusBadge status={task.status} size="sm" />
                            <span>{task.assignee || "—"}</span>
                          </div>
                        </div>
                      );
                    })
                  )}
              {tasks.length > 50 && (
                <div className="dash-empty">
                  <Link to={`/tasks?project=${name}`}>View all {tasks.length} tasks</Link>
                </div>
              )}
            </div>
          )}

          {/* Missions Tab */}
          {tab === "missions" && (
            <div>
              {missions.length === 0 ? (
                <div className="dash-empty">No missions in this project</div>
              ) : (
                <div className="cards-grid">
                  {missions.map((m: any) => (
                    <MissionCard key={m.id} mission={m} />
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Audit Tab */}
          {tab === "audit" && (
            <div className="column-section">
              <div className="column-section-body">
                {audit.length === 0 ? (
                  <div className="dash-empty">No audit events for this project</div>
                ) : (
                  audit.map((entry: any, i: number) => (
                    <AuditEntryComponent key={i} entry={entry} />
                  ))
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    </>
  );
}
