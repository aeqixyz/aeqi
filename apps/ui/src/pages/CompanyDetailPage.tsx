import { useEffect, useState } from "react";
import { useParams, Link } from "react-router-dom";
import Header from "@/components/Header";
import StatusBadge from "@/components/StatusBadge";
import MissionCard from "@/components/MissionCard";
import AuditEntryComponent from "@/components/AuditEntry";
import { PRIORITY_COLORS } from "@/lib/constants";
import { api } from "@/lib/api";
import { runtimeLabel, summarizeTaskRuntime } from "@/lib/runtime";

export default function CompanyDetailPage() {
  const { name } = useParams<{ name: string }>();
  const [company, setCompany] = useState<any>(null);
  const [tasks, setTasks] = useState<any[]>([]);
  const [missions, setMissions] = useState<any[]>([]);
  const [audit, setAudit] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState<"tasks" | "missions" | "audit">("tasks");

  useEffect(() => {
    if (!name) return;
    setLoading(true);

    Promise.all([
      api.getCompanies().then((d) => {
        const p = (d.companies || []).find((p: any) => p.name === name);
        setCompany(p || null);
      }),
      api.getTasks({ company: name }).then((d) => setTasks(d.tasks || [])),
      api.getMissions({ company: name }).then((d) => setMissions(d.missions || [])),
      api.getAudit({ company: name, last: 30 }).then((d) => setAudit(d.events || [])),
    ])
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [name]);

  if (loading) return <div className="loading">Loading company...</div>;
  if (!company) return <div className="loading">Company not found</div>;

  const pendingTasks = tasks.filter((t) => t.status === "pending");
  const activeTasks = tasks.filter((t) => t.status === "in_progress");
  const doneTasks = tasks.filter((t) => t.status === "done");
  const total = tasks.length;
  const donePct = total > 0 ? (doneTasks.length / total) * 100 : 0;

  return (
    <>
      <Header
        title={company.name}
        breadcrumbs={[
          { label: "Companies", href: "/companies" },
          { label: company.name },
        ]}
      />

      {/* Hero Stats */}
      <div className="hero-stats">
        <div className="hero-stat">
          <div className="hero-stat-value">{total}</div>
          <div className="hero-stat-label">Total Tasks</div>
        </div>
        <div className="hero-stat-divider" />
        <div className="hero-stat">
          <div className="hero-stat-value muted">{pendingTasks.length}</div>
          <div className="hero-stat-label">Pending</div>
        </div>
        <div className="hero-stat-divider" />
        <div className="hero-stat">
          <div className="hero-stat-value info">{activeTasks.length}</div>
          <div className="hero-stat-label">In Progress</div>
        </div>
        <div className="hero-stat-divider" />
        <div className="hero-stat">
          <div className="hero-stat-value success">{doneTasks.length}</div>
          <div className="hero-stat-label">Done</div>
        </div>
        <div className="hero-stat-divider" />
        <div className="hero-stat">
          <div className="hero-stat-value">{missions.length}</div>
          <div className="hero-stat-label">Missions</div>
        </div>
      </div>

      {/* Company Info */}
      <div className="detail-grid">
        <div className="detail-sidebar">
          {/* Info Panel */}
          <div className="detail-panel">
            <div className="detail-panel-title">Company Info</div>
            <div className="detail-field">
              <div className="detail-field-label">Prefix</div>
              <div className="detail-field-value"><code>{company.prefix}</code></div>
            </div>
            {company.team && (
              <>
                <div className="detail-field">
                  <div className="detail-field-label">Team Leader</div>
                  <div className="detail-field-value">{company.team.leader}</div>
                </div>
                <div className="detail-field">
                  <div className="detail-field-label">Team</div>
                  <div className="flex-wrap-tags">
                    {(company.team.agents || []).map((a: string) => (
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
              <div className="text-hint" style={{ marginTop: "4px" }}>
                {donePct.toFixed(0)}% complete
              </div>
            </div>
          </div>
        </div>

        {/* Main Content */}
        <div className="detail-main">
          {/* Tabs */}
          <div className="tab-bar">
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
                    <div className="dash-empty">No tasks in this company</div>
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
                          <div className="task-row-detail">
                            <span className="task-subject">{task.subject}</span>
                            {(label || detail) && (
                              <span className="task-runtime">
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
                  <Link to={`/tasks?company=${name}`}>View all {tasks.length} tasks</Link>
                </div>
              )}
            </div>
          )}

          {/* Missions Tab */}
          {tab === "missions" && (
            <div>
              {missions.length === 0 ? (
                <div className="dash-empty">No missions in this company</div>
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
                  <div className="dash-empty">No audit events for this company</div>
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
