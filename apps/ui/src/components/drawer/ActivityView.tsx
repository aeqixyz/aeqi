import { useState } from "react";
import { useDaemonStore } from "@/store/daemon";
import { api } from "@/lib/api";
import type { Quest, QuestPriority, QuestStatus, AuditEntry } from "@/lib/types";

function timeAgo(ts: string | undefined | null): string {
  if (!ts) return "";
  const diff = Date.now() - new Date(ts).getTime();
  if (diff < 0) return "now";
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return `${sec}s`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h`;
  return `${Math.floor(hr / 24)}d`;
}

const PRIORITY_COLORS: Record<QuestPriority, string> = {
  critical: "var(--error, #ef4444)",
  high: "var(--warning, #f59e0b)",
  normal: "var(--info, #3b82f6)",
  low: "var(--text-muted, #71717a)",
};

const STATUS_DOT_COLORS: Record<string, string> = {
  in_progress: "var(--success, #22c55e)",
  pending: "var(--info, #3b82f6)",
  blocked: "var(--error, #ef4444)",
  done: "var(--text-muted, #71717a)",
  cancelled: "var(--text-muted, #71717a)",
};

function statusLabel(status: QuestStatus): string {
  return status.replace("_", " ");
}

function parseCriteria(text: string): { checked: boolean; label: string }[] {
  return text
    .split("\n")
    .filter((line) => /^\s*-\s*\[[ xX]\]/.test(line))
    .map((line) => ({
      checked: /^\s*-\s*\[[xX]\]/.test(line),
      label: line.replace(/^\s*-\s*\[[ xX]\]\s*/, ""),
    }));
}

// --- Active Quest Card ---

function ActiveQuestCard({ quest }: { quest: Quest }) {
  const [descExpanded, setDescExpanded] = useState(false);
  const [closing, setClosing] = useState(false);

  const description = quest.description || "";
  const descLines = description.split("\n");
  const isLongDesc = descLines.length > 2;
  const descPreview = isLongDesc
    ? descLines.slice(0, 2).join("\n") + "\u2026"
    : description;

  const criteria = quest.acceptance_criteria
    ? parseCriteria(quest.acceptance_criteria)
    : [];

  const handleComplete = async () => {
    setClosing(true);
    try {
      await api.closeTask(quest.id);
    } catch {
      // silently fail -- store will refresh
    } finally {
      setClosing(false);
    }
  };

  const handleBlock = async () => {
    setClosing(true);
    try {
      await api.closeTask(quest.id, { reason: "blocked" });
    } catch {
      // silently fail
    } finally {
      setClosing(false);
    }
  };

  return (
    <div
      className="active-quest-card"
      style={{
        borderLeftColor: PRIORITY_COLORS[quest.priority] || PRIORITY_COLORS.normal,
      }}
    >
      <div className="aq-header">
        <span className="aq-subject">{quest.subject}</span>
        <span className="aq-status-badge">{statusLabel(quest.status)}</span>
      </div>

      {description && (
        <div className="aq-desc">
          <span>{descExpanded ? description : descPreview}</span>
          {isLongDesc && (
            <button
              className="aq-expand-btn"
              onClick={() => setDescExpanded((p) => !p)}
            >
              {descExpanded ? "less" : "more"}
            </button>
          )}
        </div>
      )}

      {criteria.length > 0 && (
        <ul className="aq-criteria">
          {criteria.map((c, i) => (
            <li key={i} className={c.checked ? "checked" : ""}>
              <span className="aq-check">{c.checked ? "\u2611" : "\u2610"}</span>
              <span>{c.label}</span>
            </li>
          ))}
        </ul>
      )}

      {quest.cost_usd > 0 && (
        <span className="aq-cost">${quest.cost_usd.toFixed(2)}</span>
      )}

      <div className="aq-actions">
        <button
          className="aq-action-btn complete"
          onClick={handleComplete}
          disabled={closing}
        >
          Complete
        </button>
        <button
          className="aq-action-btn block"
          onClick={handleBlock}
          disabled={closing}
        >
          Block
        </button>
      </div>
    </div>
  );
}

// --- Quest List ---

function QuestList({
  quests,
  activeQuestId,
}: {
  quests: Quest[];
  activeQuestId: string | null;
}) {
  const others = quests.filter((q) => q.id !== activeQuestId);
  if (others.length === 0) return null;

  return (
    <div className="ctx-section">
      <div className="ctx-section-title">Quests ({others.length})</div>
      <div className="ctx-list">
        {others.map((q) => (
          <div key={q.id} className="ctx-quest-row">
            <span
              className="ctx-quest-dot"
              style={{
                background: STATUS_DOT_COLORS[q.status] || STATUS_DOT_COLORS.done,
              }}
            />
            <span className="ctx-quest-subject">{q.subject}</span>
            <span className="ctx-quest-time">
              {timeAgo(q.updated_at || q.created_at)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

// --- Event Stream ---

function EventStream({ agentName }: { agentName: string }) {
  const events = useDaemonStore((s) => s.events);
  const wsConnected = useDaemonStore((s) => s.wsConnected);

  const agentEvents = events
    .filter((e: AuditEntry) => {
      const agent = (e.agent || "").toLowerCase();
      return agent.includes(agentName.toLowerCase());
    })
    .slice(0, 30);

  return (
    <div className="ctx-section">
      <div className="ctx-section-title">
        Events
        {wsConnected && <span className="ctx-live-dot" title="Live" />}
      </div>
      {agentEvents.length > 0 ? (
        <div className="ctx-list">
          {agentEvents.map((e, i) => (
            <div key={e.id || i} className="ctx-event-row">
              <span className="ctx-event-time">
                {timeAgo(e.timestamp)}
              </span>
              <span className="ctx-event-type">
                {e.decision_type || "event"}
              </span>
              <span className="ctx-event-summary">
                {e.summary || "\u2014"}
              </span>
            </div>
          ))}
        </div>
      ) : (
        <div className="ctx-empty-state">No events for this agent</div>
      )}
    </div>
  );
}

// --- Main Component ---

export default function ActivityView({
  agentName,
  agentId,
}: {
  agentName: string;
  agentId: string;
}) {
  const quests = useDaemonStore((s) => s.quests);

  // Find quests assigned to this agent
  const agentQuests = quests.filter((q: any) => {
    const assignee = (q.assignee || q.agent || q.agent_id || "").toLowerCase();
    return (
      assignee.includes(agentName.toLowerCase()) ||
      assignee === agentId.toLowerCase()
    );
  }) as Quest[];

  // Active quest: first in_progress
  const activeQuest =
    agentQuests.find((q) => q.status === "in_progress") || null;

  // Other quests: pending, blocked, recent done (sorted by update time)
  const otherQuests = agentQuests
    .filter((q) => q.id !== activeQuest?.id)
    .sort((a, b) => {
      const order: Record<string, number> = {
        pending: 0,
        blocked: 1,
        in_progress: 2,
        done: 3,
        cancelled: 4,
      };
      const oa = order[a.status] ?? 5;
      const ob = order[b.status] ?? 5;
      if (oa !== ob) return oa - ob;
      const ta = a.updated_at || a.created_at || "";
      const tb = b.updated_at || b.created_at || "";
      return tb.localeCompare(ta);
    });

  return (
    <div className="activity-view">
      {activeQuest && <ActiveQuestCard quest={activeQuest} />}

      <QuestList quests={otherQuests} activeQuestId={activeQuest?.id || null} />

      <EventStream agentName={agentName} />
    </div>
  );
}
