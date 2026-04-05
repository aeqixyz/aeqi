import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import "@/styles/quests.css";
import Header from "@/components/Header";
import { useDaemonStore } from "@/store/daemon";
import { useChatStore } from "@/store/chat";
import { api } from "@/lib/api";
import type { QuestStatus, QuestPriority } from "@/lib/types";

/* ── Helpers ──────────────────────────────────────────── */

const PRIORITY_BORDER: Record<string, string> = {
  critical: "var(--error)",
  high: "var(--warning, #f59e0b)",
  normal: "var(--accent)",
  low: "var(--text-muted)",
};

const COLUMNS: { status: QuestStatus; label: string }[] = [
  { status: "pending", label: "Pending" },
  { status: "in_progress", label: "In Progress" },
  { status: "blocked", label: "Blocked" },
  { status: "done", label: "Done" },
];

function timeAgo(iso: string): string {
  const seconds = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  return `${Math.floor(days / 30)}mo ago`;
}

/* ── Create Quest Modal ───────────────────────────────── */

interface CreateModalProps {
  open: boolean;
  defaultStatus?: QuestStatus;
  onClose: () => void;
}

function CreateQuestModal({ open, defaultStatus, onClose }: CreateModalProps) {
  const agents = useDaemonStore((s) => s.agents);
  const fetchQuests = useDaemonStore((s) => s.fetchQuests);
  const selectedAgent = useChatStore((s) => s.selectedAgent);

  const [subject, setSubject] = useState("");
  const [description, setDescription] = useState("");
  const [priority, setPriority] = useState<QuestPriority>("normal");
  const [agentName, setAgentName] = useState(selectedAgent?.name || "");
  const [acceptance, setAcceptance] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const subjectRef = useRef<HTMLInputElement>(null);

  // Reset form when opened
  useEffect(() => {
    if (open) {
      setSubject("");
      setDescription("");
      setPriority("normal");
      setAgentName(selectedAgent?.name || "");
      setAcceptance("");
      setSubmitting(false);
      setTimeout(() => subjectRef.current?.focus(), 50);
    }
  }, [open, selectedAgent]);

  const handleCreate = async () => {
    if (!subject.trim() || submitting) return;
    setSubmitting(true);
    try {
      await api.createTask({
        company: agentName || selectedAgent?.name || "default",
        subject: subject.trim(),
        description: description.trim() || undefined,
      });
      await fetchQuests();
      onClose();
    } catch {
      // allow retry
      setSubmitting(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
    }
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      handleCreate();
    }
  };

  if (!open) return null;

  const priorities: QuestPriority[] = ["critical", "high", "normal", "low"];

  return (
    <div className="cq-backdrop" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="cq-modal" onKeyDown={handleKeyDown}>
        <div className="cq-title">
          New Quest{defaultStatus && defaultStatus !== "pending" ? ` — ${defaultStatus.replace("_", " ")}` : ""}
        </div>

        <div className="cq-field">
          <label className="cq-label">Subject</label>
          <input
            ref={subjectRef}
            className="cq-input"
            placeholder="What needs to be done?"
            value={subject}
            onChange={(e) => setSubject(e.target.value)}
          />
        </div>

        <div className="cq-field">
          <label className="cq-label">Description</label>
          <textarea
            className="cq-textarea"
            placeholder="Optional details..."
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={2}
          />
        </div>

        <div className="cq-field">
          <label className="cq-label">Priority</label>
          <div className="cq-priority-group">
            {priorities.map((p) => (
              <button
                key={p}
                className={`cq-priority-btn${priority === p ? " selected" : ""}`}
                style={priority === p ? { borderColor: PRIORITY_BORDER[p] } : undefined}
                onClick={() => setPriority(p)}
                type="button"
              >
                {p}
              </button>
            ))}
          </div>
        </div>

        <div className="cq-field">
          <label className="cq-label">Assign to agent</label>
          <select
            className="cq-select"
            value={agentName}
            onChange={(e) => setAgentName(e.target.value)}
          >
            <option value="">Unassigned</option>
            {agents.map((a) => (
              <option key={a.id} value={a.name}>
                {a.display_name || a.name}
              </option>
            ))}
          </select>
        </div>

        <div className="cq-field">
          <label className="cq-label">Acceptance criteria</label>
          <textarea
            className="cq-textarea"
            placeholder="Define what done looks like..."
            value={acceptance}
            onChange={(e) => setAcceptance(e.target.value)}
            rows={2}
          />
        </div>

        <div className="cq-actions">
          <button className="cq-btn cq-btn-cancel" onClick={onClose} type="button">
            Cancel
          </button>
          <button
            className="cq-btn cq-btn-create"
            onClick={handleCreate}
            disabled={!subject.trim() || submitting}
            type="button"
          >
            {submitting ? "Creating..." : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}

/* ── Main Page ────────────────────────────────────────── */

export default function QuestsPage() {
  const quests = useDaemonStore((s) => s.quests);
  const agents = useDaemonStore((s) => s.agents);
  const selectedAgent = useChatStore((s) => s.selectedAgent);

  const [search, setSearch] = useState("");
  const [agentFilter, setAgentFilter] = useState("");
  const [showDone, setShowDone] = useState(true);
  const [modalOpen, setModalOpen] = useState(false);
  const [modalDefaultStatus, setModalDefaultStatus] = useState<QuestStatus>("pending");

  // Keyboard shortcut: c or Cmd+N to open modal
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Ignore if user is typing in an input/textarea
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

      if (e.key === "c" && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        setModalDefaultStatus("pending");
        setModalOpen(true);
        return;
      }
      if (e.key === "n" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setModalDefaultStatus("pending");
        setModalOpen(true);
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  // Filter quests
  const filtered = useMemo(() => {
    let result = quests;

    // Agent filter (dropdown or sidebar selection)
    const effectiveAgent = agentFilter || (selectedAgent ? selectedAgent.name : "");
    if (effectiveAgent) {
      result = result.filter(
        (q: any) =>
          q.assignee === effectiveAgent ||
          q.agent_id === effectiveAgent
      );
    }

    // Search filter
    if (search.trim()) {
      const term = search.toLowerCase();
      result = result.filter((q: any) =>
        q.subject?.toLowerCase().includes(term)
      );
    }

    return result;
  }, [quests, agentFilter, selectedAgent, search]);

  // Group into columns
  const columns = useMemo(() => {
    const groups: Record<string, any[]> = {
      pending: [],
      in_progress: [],
      blocked: [],
      done: [],
    };

    for (const q of filtered) {
      const status = q.status as string;
      if (status === "cancelled" || status === "done") {
        groups.done.push(q);
      } else if (groups[status]) {
        groups[status].push(q);
      } else {
        groups.pending.push(q);
      }
    }

    // Sort done by updated_at/created_at desc, limit to 10
    groups.done.sort((a: any, b: any) => {
      const ta = new Date(a.updated_at || a.created_at).getTime();
      const tb = new Date(b.updated_at || b.created_at).getTime();
      return tb - ta;
    });
    groups.done = groups.done.slice(0, 10);

    return groups;
  }, [filtered]);

  const openCreateForStatus = useCallback((status: QuestStatus) => {
    setModalDefaultStatus(status);
    setModalOpen(true);
  }, []);

  return (
    <div className="page-content kb-page">
      <Header
        title="Quests"
        actions={
          <button className="kb-btn-new" onClick={() => openCreateForStatus("pending")}>
            + New Quest<span className="kb-kbd">C</span>
          </button>
        }
      />

      {/* Filter bar */}
      <div className="kb-toolbar">
        <div className="kb-toolbar-left">
          <input
            className="kb-search"
            placeholder="Search quests..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <select
            className="kb-filter-select"
            value={agentFilter}
            onChange={(e) => setAgentFilter(e.target.value)}
          >
            <option value="">All agents</option>
            {agents.map((a) => (
              <option key={a.id} value={a.name}>
                {a.display_name || a.name}
              </option>
            ))}
          </select>
          <button
            className={`kb-toggle${showDone ? " active" : ""}`}
            onClick={() => setShowDone((v) => !v)}
            type="button"
          >
            {showDone ? "Hide" : "Show"} Done
          </button>
        </div>
      </div>

      {/* Kanban board */}
      <div className="kb-board">
        {COLUMNS.map((col) => {
          if (col.status === "done" && !showDone) return null;
          const cards = columns[col.status] || [];
          return (
            <div className="kb-column" key={col.status}>
              <div className="kb-col-header">
                <span className="kb-col-title">{col.label}</span>
                <span className="kb-col-count">{cards.length}</span>
                <button
                  className="kb-col-add"
                  title={`Create quest in ${col.label}`}
                  onClick={() => openCreateForStatus(col.status)}
                  type="button"
                >
                  +
                </button>
              </div>
              <div className="kb-col-body">
                {cards.length === 0 && (
                  <div className="kb-col-empty">No quests</div>
                )}
                {cards.map((q: any) => (
                  <div
                    key={q.id}
                    className={`kb-card${q.status === "cancelled" ? " cancelled" : ""}`}
                    style={{ borderLeftColor: PRIORITY_BORDER[q.priority] || "var(--text-muted)" }}
                  >
                    <div className="kb-card-subject">{q.subject}</div>
                    <div className="kb-card-meta">
                      {q.assignee && (
                        <span className="kb-card-assignee">{q.assignee}</span>
                      )}
                      {q.assignee && q.created_at && (
                        <span className="kb-card-dot">&middot;</span>
                      )}
                      {q.created_at && (
                        <span className="kb-card-time">{timeAgo(q.created_at)}</span>
                      )}
                    </div>
                    {q.labels && q.labels.length > 0 && (
                      <div className="kb-card-labels">
                        {q.labels.map((l: string) => (
                          <span key={l} className="kb-card-label">{l}</span>
                        ))}
                      </div>
                    )}
                  </div>
                ))}
              </div>
            </div>
          );
        })}
      </div>

      {/* Create modal */}
      <CreateQuestModal
        open={modalOpen}
        defaultStatus={modalDefaultStatus}
        onClose={() => setModalOpen(false)}
      />
    </div>
  );
}
