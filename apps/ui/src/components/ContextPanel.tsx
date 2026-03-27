import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useChatStore } from "@/store/chat";
import { api } from "@/lib/api";
import { runtimeLabel } from "@/lib/runtime";

function NotesTab({ channel }: { channel: string | null }) {
  const [content, setContent] = useState("");
  const [directives, setDirectives] = useState<any[]>([]);
  const [saving, setSaving] = useState(false);
  const saveTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const channelKey = channel || "sigil";

  // Load note on channel change
  useEffect(() => {
    api.getNote(channelKey).then((d) => {
      if (d.ok && d.note) {
        setContent(d.note.content || "");
        setDirectives(d.directives || []);
      } else {
        setContent("");
        setDirectives([]);
      }
    }).catch(() => {});
  }, [channelKey]);

  // Auto-save on debounce (1.5s)
  const handleChange = (value: string) => {
    setContent(value);
    if (saveTimeoutRef.current) clearTimeout(saveTimeoutRef.current);
    saveTimeoutRef.current = setTimeout(() => {
      setSaving(true);
      api.saveNote({ channel: channelKey, content: value }).then((d) => {
        if (d.ok && d.directives) setDirectives(d.directives);
        setSaving(false);
      }).catch(() => setSaving(false));
    }, 1500);
  };

  return (
    <div className="ctx-content">
      <div className="ctx-section">
        <div className="ctx-section-header">
          <span className="ctx-section-title">Notes</span>
          {saving && <span className="ctx-link">saving...</span>}
        </div>
        <textarea
          className="ctx-notes-editor"
          value={content}
          onChange={(e) => handleChange(e.target.value)}
          placeholder="Write your directives here..."
          rows={8}
        />
      </div>
      {directives.length > 0 && (
        <div className="ctx-section">
          <div className="ctx-section-title">Directives</div>
          <div className="ctx-list">
            {directives.map((d: any) => (
              <div key={d.id} className="ctx-directive">
                <span className={`ctx-directive-status ctx-directive-${d.status}`}>
                  {d.status === "pending" ? "\u25CB" : d.status === "active" ? "\u27F3" : d.status === "done" ? "\u2713" : "\u2717"}
                </span>
                <span className="ctx-directive-text">{d.content}</span>
                {d.task_id && <code className="ctx-directive-task">{d.task_id}</code>}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function BriefTab() {
  const [brief, setBrief] = useState<string | null>(null);

  useEffect(() => {
    api.getBrief().then((d) => setBrief(d.brief || null)).catch(() => {});
  }, []);

  return (
    <div className="ctx-content">
      {brief ? (
        <div className="ctx-section">
          <div className="ctx-section-title">Daily Brief</div>
          <pre className="ctx-brief">{brief}</pre>
        </div>
      ) : (
        <div className="ctx-empty">No brief available</div>
      )}
    </div>
  );
}

function GlobalContext() {
  const [brief, setBrief] = useState<string | null>(null);
  const [tasks, setTasks] = useState<any[]>([]);
  const [audit, setAudit] = useState<any[]>([]);
  const [cost, setCost] = useState<any>(null);

  useEffect(() => {
    api.getBrief().then((d) => setBrief(d.brief || null)).catch(() => {});
    api.getTasks({ status: "in_progress" }).then((d) => setTasks(d.tasks || [])).catch(() => {});
    api.getAudit({ last: 12 }).then((d) => setAudit(d.events || [])).catch(() => {});
    api.getCost().then(setCost).catch(() => {});
    const interval = setInterval(() => {
      api.getTasks({ status: "in_progress" }).then((d) => setTasks(d.tasks || [])).catch(() => {});
      api.getAudit({ last: 12 }).then((d) => setAudit(d.events || [])).catch(() => {});
    }, 12000);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="ctx-content">
      {brief && (
        <div className="ctx-section">
          <div className="ctx-section-title">Daily Brief</div>
          <pre className="ctx-brief">{brief}</pre>
        </div>
      )}

      {cost && (
        <div className="ctx-section">
          <div className="ctx-section-title">Budget</div>
          <div className="ctx-budget">
            <div className="ctx-budget-bar">
              <div
                className="ctx-budget-fill"
                style={{ width: `${Math.min((cost.spent_today_usd / (cost.daily_budget_usd || 1)) * 100, 100)}%` }}
              />
            </div>
            <span className="ctx-budget-label">
              ${Number(cost.spent_today_usd).toFixed(2)} / ${Number(cost.daily_budget_usd).toFixed(0)}
            </span>
          </div>
        </div>
      )}

      <div className="ctx-section">
        <div className="ctx-section-header">
          <span className="ctx-section-title">Active Work</span>
          <Link to="/tasks" className="ctx-link">{tasks.length}</Link>
        </div>
        {tasks.length === 0 ? (
          <div className="ctx-empty">No active tasks</div>
        ) : (
          <div className="ctx-list">
            {tasks.slice(0, 6).map((t: any) => (
              <div key={t.id} className="ctx-task">
                <code className="ctx-task-id">{t.id}</code>
                <span className="ctx-task-subject">{t.subject}</span>
                {runtimeLabel(t.runtime) && (
                  <span className="ctx-task-status">{runtimeLabel(t.runtime)}</span>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="ctx-section">
        <div className="ctx-section-header">
          <span className="ctx-section-title">
            <span className="ctx-feed-dot" />
            Activity
          </span>
          <Link to="/audit" className="ctx-link">All</Link>
        </div>
        <div className="ctx-list">
          {audit.slice(0, 8).map((e: any, i: number) => (
            <div key={i} className="ctx-event">
              <span className="ctx-event-type">{e.decision_type?.replace(/_/g, " ")}</span>
              <span className="ctx-event-detail">{e.reasoning?.slice(0, 60)}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function ProjectContext({ project }: { project: string }) {
  const [tasks, setTasks] = useState<any[]>([]);
  const [knowledge, setKnowledge] = useState<any[]>([]);
  const [projectData, setProjectData] = useState<any>(null);
  const [search, setSearch] = useState("");

  useEffect(() => {
    setSearch("");
    api.getTasks({ project }).then((d) => setTasks(d.tasks || [])).catch(() => {});
    api.getChannelKnowledge({ project, limit: 15 }).then((d) => setKnowledge(d.items || [])).catch(() => {});
    api.getProjects().then((d) => {
      const p = (d.projects || []).find((p: any) => p.name === project);
      setProjectData(p || null);
    }).catch(() => {});
  }, [project]);

  const handleSearch = (q: string) => {
    setSearch(q);
    api.getChannelKnowledge({ project, query: q || undefined, limit: 15 })
      .then((d) => setKnowledge(d.items || []))
      .catch(() => {});
  };

  const openTasks = tasks.filter((t: any) => t.status === "pending" || t.status === "in_progress");
  const team = projectData?.team;

  return (
    <div className="ctx-content">
      {/* Team */}
      {team && (
        <div className="ctx-section">
          <div className="ctx-section-title">Team</div>
          <div className="ctx-team">
            {[team.leader, ...(team.agents || [])].map((name: string) => (
              <Link key={name} to={`/agents/${name}`} className="ctx-team-member">
                <span className="ctx-team-avatar">{name[0].toUpperCase()}</span>
                <span className="ctx-team-name">{name}</span>
              </Link>
            ))}
          </div>
        </div>
      )}

      {/* Tasks */}
      <div className="ctx-section">
        <div className="ctx-section-header">
          <span className="ctx-section-title">Tasks</span>
          <Link to="/tasks" className="ctx-link">{openTasks.length} open</Link>
        </div>
        {openTasks.length === 0 ? (
          <div className="ctx-empty">No open tasks</div>
        ) : (
          <div className="ctx-list">
            {openTasks.slice(0, 8).map((t: any) => (
              <div key={t.id} className="ctx-task">
                <code className="ctx-task-id">{t.id}</code>
                <span className="ctx-task-subject">{t.subject}</span>
                <span className={`ctx-task-status ctx-task-status-${t.status}`}>
                  {runtimeLabel(t.runtime) || (t.status === "in_progress" ? "active" : t.status)}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Knowledge */}
      <div className="ctx-section">
        <div className="ctx-section-header">
          <span className="ctx-section-title">Knowledge</span>
          <span className="ctx-link">{knowledge.length}</span>
        </div>
        <input
          className="ctx-search"
          placeholder="Search knowledge..."
          value={search}
          onChange={(e) => handleSearch(e.target.value)}
        />
        <div className="ctx-list">
          {knowledge.map((item: any) => (
            <div key={item.id} className="ctx-knowledge">
              <span className="ctx-knowledge-source" style={{
                color: item.source === "memory" ? "var(--info)" : "var(--accent)"
              }}>
                {item.source === "memory" ? "M" : "B"}
              </span>
              <div className="ctx-knowledge-body">
                <span className="ctx-knowledge-key">{item.key}</span>
                <span className="ctx-knowledge-preview">{item.content?.slice(0, 60)}</span>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export default function ContextPanel() {
  const channel = useChatStore((s) => s.channel);
  const projectName = channel?.split("/")[0] || null;
  const [tab, setTab] = useState<"notes" | "context" | "brief">("context");

  return (
    <aside className="context-panel">
      <div className="context-panel-header">
        <button className={`ctx-tab ${tab === "notes" ? "ctx-tab-active" : ""}`} onClick={() => setTab("notes")}>Notes</button>
        <button className={`ctx-tab ${tab === "context" ? "ctx-tab-active" : ""}`} onClick={() => setTab("context")}>Context</button>
        <button className={`ctx-tab ${tab === "brief" ? "ctx-tab-active" : ""}`} onClick={() => setTab("brief")}>Brief</button>
      </div>
      {tab === "notes" && <NotesTab channel={channel} />}
      {tab === "context" && (
        projectName ? (
          <ProjectContext project={projectName} />
        ) : (
          <GlobalContext />
        )
      )}
      {tab === "brief" && <BriefTab />}
    </aside>
  );
}
