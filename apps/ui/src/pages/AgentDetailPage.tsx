import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import Header from "@/components/Header";
import StatusBadge from "@/components/StatusBadge";
import AuditEntryComponent from "@/components/AuditEntry";
import { PRIORITY_COLORS } from "@/lib/constants";
import { api } from "@/lib/api";

export default function AgentDetailPage() {
  const { name } = useParams<{ name: string }>();
  const [agent, setAgent] = useState<any>(null);
  const [tasks, setTasks] = useState<any[]>([]);
  const [audit, setAudit] = useState<any[]>([]);
  const [identity, setIdentity] = useState<Record<string, string>>({});
  const [activeFile, setActiveFile] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [editContent, setEditContent] = useState("");
  const [saving, setSaving] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!name) return;
    setLoading(true);

    Promise.all([
      api.getAgents().then((d) => {
        const a = (d.agents || []).find((a: any) => a.name === name);
        setAgent(a || null);
      }),
      api.getAgentIdentity(name).then((d) => {
        if (d.ok && d.files) {
          setIdentity(d.files);
          const firstFile = Object.keys(d.files).find(f => f === "PERSONA.md") || Object.keys(d.files)[0];
          if (firstFile) setActiveFile(firstFile);
        }
      }).catch(() => {}),
      api.getTasks({}).then((d) => {
        const agentTasks = (d.tasks || []).filter((t: any) => t.assignee === name);
        setTasks(agentTasks);
      }),
      api.getAudit({ last: 50 }).then((d) => {
        const agentAudit = (d.events || []).filter((e: any) => e.agent?.includes(name));
        setAudit(agentAudit);
      }),
    ])
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [name]);

  if (loading) return <div className="loading">Loading agent...</div>;
  if (!agent) return <div className="loading">Agent not found</div>;

  const completedTasks = tasks.filter((t) => t.status === "done").length;
  const failedTasks = tasks.filter((t) => t.status === "cancelled").length;
  const activeTasks = tasks.filter((t) => t.status === "in_progress" || t.status === "pending");

  return (
    <>
      <Header
        title={agent.name}
        breadcrumbs={[
          { label: "Agents", href: "/agents" },
          { label: agent.name },
        ]}
      />

      {/* Hero */}
      <div className="hero-stats">
        <div className="hero-stat">
          <div className="hero-stat-value">{tasks.length}</div>
          <div className="hero-stat-label">Total Tasks</div>
        </div>
        <div className="hero-stat-divider" />
        <div className="hero-stat">
          <div className="hero-stat-value success">{completedTasks}</div>
          <div className="hero-stat-label">Completed</div>
        </div>
        <div className="hero-stat-divider" />
        <div className="hero-stat">
          <div className="hero-stat-value error">{failedTasks}</div>
          <div className="hero-stat-label">Failed</div>
        </div>
        <div className="hero-stat-divider" />
        <div className="hero-stat">
          <div className="hero-stat-value">{audit.length}</div>
          <div className="hero-stat-label">Decisions</div>
        </div>
      </div>

      <div className="detail-grid">
        <div className="detail-sidebar">
          {/* Identity Panel */}
          <div className="detail-panel">
            <div className="detail-panel-title">Identity</div>
            <div className="detail-field">
              <div className="detail-field-label">Role</div>
              <div className="detail-field-value">{agent.role}</div>
            </div>
            <div className="detail-field">
              <div className="detail-field-label">Prefix</div>
              <div className="detail-field-value"><code>{agent.prefix}</code></div>
            </div>
            <div className="detail-field">
              <div className="detail-field-label">Model</div>
              <div className="detail-field-value">{agent.model || "default"}</div>
            </div>
          </div>

          {/* Expertise Panel */}
          <div className="detail-panel">
            <div className="detail-panel-title">Expertise</div>
            <div className="flex-wrap-tags">
              {(agent.expertise || []).map((e: string) => (
                <span key={e} className="expertise-tag">{e}</span>
              ))}
              {(!agent.expertise || agent.expertise.length === 0) && (
                <span className="text-hint">No expertise tags</span>
              )}
            </div>
            {agent.expertise_scores && agent.expertise_scores.length > 0 && (
              <div style={{ marginTop: "var(--space-4)" }}>
                <div className="detail-field-label" style={{ marginBottom: "var(--space-2)" }}>Scores</div>
                {agent.expertise_scores.map((s: any, i: number) => (
                  <div key={i} style={{ display: "flex", justifyContent: "space-between", fontSize: "var(--font-size-xs)", color: "var(--text-secondary)", marginBottom: "4px" }}>
                    <span>{(s.success_rate * 100).toFixed(0)}% success</span>
                    <span>{s.total_tasks} tasks</span>
                    <span>{(s.confidence * 100).toFixed(0)}% conf</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Main Content */}
        <div className="detail-main">
          {/* Identity Files — File Explorer + Editor */}
          {Object.keys(identity).length > 0 && (
            <div className="dash-panel">
              <div className="dash-panel-header">
                <span className="dash-panel-title">
                  {activeFile || "Identity"}
                </span>
                <div style={{ display: "flex", gap: "var(--space-2)", alignItems: "center" }}>
                  {activeFile && !editing && (
                    <button
                      className="btn btn-xs"
                      onClick={() => { setEditing(true); setEditContent(identity[activeFile]); }}
                    >
                      Edit
                    </button>
                  )}
                  {editing && (
                    <>
                      <button
                        className="btn btn-primary btn-xs"
                        disabled={saving}
                        onClick={async () => {
                          if (!name || !activeFile) return;
                          setSaving(true);
                          try {
                            await api.saveAgentFile(name, activeFile, editContent);
                            setIdentity({ ...identity, [activeFile]: editContent });
                            setEditing(false);
                          } catch { /* ignore */ }
                          setSaving(false);
                        }}
                      >
                        {saving ? "Saving..." : "Save"}
                      </button>
                      <button
                        className="btn btn-xs"
                        onClick={() => setEditing(false)}
                      >
                        Cancel
                      </button>
                    </>
                  )}
                </div>
              </div>
              {/* File tabs */}
              <div className="file-tabs">
                {Object.keys(identity).map((filename) => (
                  <button
                    key={filename}
                    className={`file-tab${activeFile === filename ? " active" : ""}`}
                    onClick={() => { setActiveFile(filename); setEditing(false); }}
                  >
                    {filename}
                  </button>
                ))}
              </div>
              {/* Content */}
              {activeFile && identity[activeFile] != null && (
                <div style={{ position: "relative" }}>
                  {editing ? (
                    <textarea
                      className="code-editor"
                      value={editContent}
                      onChange={(e) => setEditContent(e.target.value)}
                    />
                  ) : (
                    <pre className="code-viewer">
                      {identity[activeFile]}
                    </pre>
                  )}
                </div>
              )}
            </div>
          )}

          {/* Active Work */}
          {activeTasks.length > 0 && (
            <div className="dash-panel">
              <div className="dash-panel-header">
                <span className="dash-panel-title">Active Work</span>
                <span className="text-hint">{activeTasks.length} tasks</span>
              </div>
              <div className="task-table">
                {activeTasks.map((task: any) => (
                  <div key={task.id} className="task-row">
                    <span className="task-priority-bar" style={{ backgroundColor: PRIORITY_COLORS[task.priority] || "var(--text-primary)" }} />
                    <code className="task-id">{task.id}</code>
                    <span className="task-subject">{task.subject}</span>
                    <div className="task-meta">
                      <StatusBadge status={task.status} size="sm" />
                      <span>{task.company}</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Recent Activity */}
          <div className="dash-panel">
            <div className="dash-panel-header">
              <span className="dash-panel-title">Recent Activity</span>
              <span className="text-hint">{audit.length} events</span>
            </div>
            <div className="column-section-body">
              {audit.length === 0 ? (
                <div className="dash-empty">No recent activity</div>
              ) : (
                audit.slice(0, 20).map((entry: any, i: number) => (
                  <AuditEntryComponent key={i} entry={entry} compact />
                ))
              )}
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
