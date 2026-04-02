import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import { useChatStore } from "@/store/chat";

export default function DashboardPage() {
  const selectedAgent = useChatStore((s) => s.selectedAgent);
  const channel = useChatStore((s) => s.channel);
  const [status, setStatus] = useState<any>(null);
  const [agents, setAgents] = useState<any[]>([]);
  const [tasks, setTasks] = useState<any[]>([]);
  const [triggers, setTriggers] = useState<any[]>([]);
  const [audit, setAudit] = useState<any[]>([]);

  useEffect(() => {
    api.getStatus().then(setStatus).catch(() => {});
    api.getAgents().then((d: any) => setAgents(d.agents || [])).catch(() => {});
    api.getTasks({}).then((d: any) => setTasks(d.tasks || [])).catch(() => {});
    api.getAudit({ last: 10 }).then((d: any) => setAudit(d.events || d.audit || [])).catch(() => {});
    api.getCrons?.().then((d: any) => setTriggers(d.triggers || d.crons || [])).catch(() => {});
  }, [selectedAgent, channel]);

  const activeAgents = agents.filter((a) => a.status === "active" || a.status === "Active");
  const pendingTasks = tasks.filter((t) => t.status === "Pending" || t.status === "pending");
  const activeTasks = tasks.filter((t) => t.status === "InProgress" || t.status === "in_progress");
  const spent = status?.cost_today_usd ?? 0;
  const budget = status?.daily_budget_usd ?? 0;

  // Determine what we're showing
  const scopeLabel = selectedAgent
    ? (selectedAgent.startsWith("dept:") ? "Department" : selectedAgent)
    : (channel || "AEQI");

  return (
    <div className="home-page">
      <div className="home-header">
        <h1 className="home-title">{scopeLabel}</h1>
        <p className="home-meta">
          {activeAgents.length} agents · {tasks.length} tasks · ${spent.toFixed(2)} spent today
        </p>
      </div>

      {/* Stats grid */}
      <div className="home-stats">
        <div className="home-stat">
          <span className="home-stat-value">{activeAgents.length}</span>
          <span className="home-stat-label">agents</span>
        </div>
        <div className="home-stat">
          <span className="home-stat-value">{pendingTasks.length}</span>
          <span className="home-stat-label">pending</span>
        </div>
        <div className="home-stat">
          <span className="home-stat-value">{activeTasks.length}</span>
          <span className="home-stat-label">active</span>
        </div>
        <div className="home-stat">
          <span className="home-stat-value">${spent.toFixed(2)}</span>
          <span className="home-stat-label">spent</span>
        </div>
        <div className="home-stat">
          <span className="home-stat-value">${budget.toFixed(2)}</span>
          <span className="home-stat-label">budget</span>
        </div>
        <div className="home-stat">
          <span className="home-stat-value">{triggers.length}</span>
          <span className="home-stat-label">triggers</span>
        </div>
      </div>

      {/* Recent activity */}
      {audit.length > 0 && (
        <div className="home-section">
          <h3 className="home-section-title">Recent Activity</h3>
          <div className="home-activity">
            {audit.slice(0, 8).map((e: any, i: number) => (
              <div key={i} className="home-activity-row">
                <span className="home-activity-type">{e.decision_type || e.type || "event"}</span>
                <span className="home-activity-text">{e.reasoning || e.summary || e.description || "—"}</span>
                <span className="home-activity-time">
                  {e.timestamp ? new Date(e.timestamp).toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit" }) : ""}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Active work */}
      {activeTasks.length > 0 && (
        <div className="home-section">
          <h3 className="home-section-title">Active Work</h3>
          <div className="home-tasks">
            {activeTasks.map((t: any) => (
              <div key={t.id} className="home-task-row">
                <span className="home-task-id">{t.id}</span>
                <span className="home-task-subject">{t.subject}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
