import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import "@/styles/quests.css";

import { useDaemonStore } from "@/store/daemon";
import { useChatStore } from "@/store/chat";
import { api } from "@/lib/api";
import { timeAgo } from "@/lib/format";
import type { Quest, QuestStatus, QuestPriority } from "@/lib/types";

/* ── Icons ───────────────────────────────────────────── */

function StatusDot({ status }: { status: QuestStatus }) {
  return <span className={`q-status-dot q-status-${status}`} />;
}

function PriorityIcon({ priority }: { priority: QuestPriority }) {
  if (priority === "critical")
    return (
      <svg className="q-priority-icon q-priority-critical" viewBox="0 0 16 16" fill="none">
        <path d="M8 3v6M8 11.5v1" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
      </svg>
    );
  if (priority === "high")
    return (
      <svg className="q-priority-icon q-priority-high" viewBox="0 0 16 16" fill="none">
        <path d="M4 10l4-4 4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    );
  if (priority === "low")
    return (
      <svg className="q-priority-icon q-priority-low" viewBox="0 0 16 16" fill="none">
        <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    );
  // normal — horizontal bars
  return (
    <svg className="q-priority-icon q-priority-normal" viewBox="0 0 16 16" fill="none">
      <path d="M4 6h8M4 10h8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

/* ── Helpers ──────────────────────────────────────────── */

const STATUS_ORDER: QuestStatus[] = ["in_progress", "pending", "blocked", "done", "cancelled"];

const STATUS_LABELS: Record<QuestStatus, string> = {
  in_progress: "In Progress",
  pending: "Pending",
  blocked: "Blocked",
  done: "Done",
  cancelled: "Cancelled",
};

interface QuestGroup {
  status: QuestStatus;
  label: string;
  quests: Quest[];
}

/* ── Create Quest Modal ───────────────────────────────── */

interface CreateModalProps {
  open: boolean;
  onClose: () => void;
}

function CreateQuestModal({ open, onClose }: CreateModalProps) {
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
        priority,
        acceptance_criteria: acceptance.trim() || undefined,
        assignee: agentName || undefined,
      });
      await fetchQuests();
      onClose();
    } catch {
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
    <div className="q-modal-backdrop" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="q-modal" onKeyDown={handleKeyDown}>
        <div className="q-modal-header">New Quest</div>

        <div className="q-modal-body">
          <input
            ref={subjectRef}
            className="q-modal-title-input"
            placeholder="Quest title"
            value={subject}
            onChange={(e) => setSubject(e.target.value)}
          />

          <textarea
            className="q-modal-desc-input"
            placeholder="Add description..."
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={3}
          />

          <div className="q-modal-fields">
            <div className="q-modal-field">
              <span className="q-modal-field-label">Priority</span>
              <div className="q-modal-priority-group">
                {priorities.map((p) => (
                  <button
                    key={p}
                    className={`q-modal-priority-btn${priority === p ? " active" : ""}`}
                    data-priority={p}
                    onClick={() => setPriority(p)}
                    type="button"
                  >
                    <PriorityIcon priority={p} />
                    <span>{p}</span>
                  </button>
                ))}
              </div>
            </div>

            <div className="q-modal-field">
              <span className="q-modal-field-label">Assignee</span>
              <select
                className="q-modal-select"
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

            <div className="q-modal-field">
              <span className="q-modal-field-label">Acceptance criteria</span>
              <textarea
                className="q-modal-desc-input"
                placeholder="Define what done looks like..."
                value={acceptance}
                onChange={(e) => setAcceptance(e.target.value)}
                rows={2}
              />
            </div>
          </div>
        </div>

        <div className="q-modal-footer">
          <button className="q-btn q-btn-ghost" onClick={onClose} type="button">
            Cancel
          </button>
          <button
            className="q-btn q-btn-primary"
            onClick={handleCreate}
            disabled={!subject.trim() || submitting}
            type="button"
          >
            {submitting ? "Creating..." : "Create quest"}
          </button>
        </div>
      </div>
    </div>
  );
}

/* ── Quest Row ───────────────────────────────────────── */

function QuestRow({ quest }: { quest: Quest }) {
  const isClosed = quest.status === "done" || quest.status === "cancelled";

  return (
    <div className={`q-row${isClosed ? " q-row-closed" : ""}`}>
      <div className="q-row-status">
        <StatusDot status={quest.status} />
      </div>
      <div className="q-row-priority">
        <PriorityIcon priority={quest.priority} />
      </div>
      <div className="q-row-id">{quest.id}</div>
      <div className="q-row-subject">
        <span className={`q-row-title${quest.status === "cancelled" ? " q-struck" : ""}`}>
          {quest.subject}
        </span>
      </div>
      {quest.labels && quest.labels.length > 0 && (
        <div className="q-row-labels">
          {quest.labels.map((l) => (
            <span key={l} className="q-label">{l}</span>
          ))}
        </div>
      )}
      <div className="q-row-spacer" />
      {quest.assignee && (
        <div className="q-row-assignee">{quest.assignee}</div>
      )}
      <div className="q-row-time">{timeAgo(quest.updated_at || quest.created_at)}</div>
    </div>
  );
}

/* ── Collapsible Group ───────────────────────────────── */

function QuestGroupSection({ group, defaultOpen }: { group: QuestGroup; defaultOpen: boolean }) {
  const [open, setOpen] = useState(defaultOpen);

  if (group.quests.length === 0) return null;

  return (
    <div className="q-group">
      <button className="q-group-header" onClick={() => setOpen((v) => !v)} type="button">
        <svg className={`q-group-chevron${open ? " open" : ""}`} viewBox="0 0 16 16" fill="none">
          <path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
        <StatusDot status={group.status} />
        <span className="q-group-label">{group.label}</span>
        <span className="q-group-count">{group.quests.length}</span>
      </button>
      {open && (
        <div className="q-group-body">
          {group.quests.map((q) => (
            <QuestRow key={q.id} quest={q} />
          ))}
        </div>
      )}
    </div>
  );
}

/* ── Filter Bar ──────────────────────────────────────── */

type ViewFilter = "all" | "active" | "closed";

/* ── Main Page ────────────────────────────────────────── */

export default function QuestsPage() {
  const quests = useDaemonStore((s) => s.quests);
  const agents = useDaemonStore((s) => s.agents);
  const selectedAgent = useChatStore((s) => s.selectedAgent);

  const [search, setSearch] = useState("");
  const [agentFilter, setAgentFilter] = useState("");
  const [viewFilter, setViewFilter] = useState<ViewFilter>("active");
  const [modalOpen, setModalOpen] = useState(false);

  // Keyboard shortcut: c or Cmd+N
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

      if (e.key === "c" && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        setModalOpen(true);
        return;
      }
      if (e.key === "n" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setModalOpen(true);
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  // Filter
  const filtered = useMemo(() => {
    let result = quests as Quest[];

    const effectiveAgent = agentFilter || (selectedAgent ? selectedAgent.name : "");
    if (effectiveAgent) {
      result = result.filter(
        (q) => q.assignee === effectiveAgent || q.agent_id === effectiveAgent,
      );
    }

    if (search.trim()) {
      const term = search.toLowerCase();
      result = result.filter((q) =>
        q.subject?.toLowerCase().includes(term) ||
        q.id?.toLowerCase().includes(term),
      );
    }

    if (viewFilter === "active") {
      result = result.filter((q) => q.status !== "done" && q.status !== "cancelled");
    } else if (viewFilter === "closed") {
      result = result.filter((q) => q.status === "done" || q.status === "cancelled");
    }

    return result;
  }, [quests, agentFilter, selectedAgent, search, viewFilter]);

  // Group by status
  const groups: QuestGroup[] = useMemo(() => {
    const map = new Map<QuestStatus, Quest[]>();
    for (const s of STATUS_ORDER) map.set(s, []);

    for (const q of filtered) {
      const list = map.get(q.status);
      if (list) list.push(q);
      else map.get("pending")!.push(q);
    }

    // Sort within groups: priority desc, then created_at desc
    const priorityWeight: Record<string, number> = { critical: 3, high: 2, normal: 1, low: 0 };
    for (const [, list] of map) {
      list.sort((a, b) => {
        const pw = (priorityWeight[b.priority] ?? 1) - (priorityWeight[a.priority] ?? 1);
        if (pw !== 0) return pw;
        return new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
      });
    }

    // Limit done/cancelled
    const done = map.get("done")!;
    if (done.length > 20) map.set("done", done.slice(0, 20));
    const cancelled = map.get("cancelled")!;
    if (cancelled.length > 10) map.set("cancelled", cancelled.slice(0, 10));

    return STATUS_ORDER.map((s) => ({
      status: s,
      label: STATUS_LABELS[s],
      quests: map.get(s) || [],
    })).filter((g) => g.quests.length > 0);
  }, [filtered]);

  const totalActive = useMemo(
    () => (quests as Quest[]).filter((q) => q.status !== "done" && q.status !== "cancelled").length,
    [quests],
  );

  // Stats
  const stats = useMemo(() => {
    const all = quests as Quest[];
    const inProgress = all.filter((q) => q.status === "in_progress").length;
    const pending = all.filter((q) => q.status === "pending").length;
    const blocked = all.filter((q) => q.status === "blocked").length;
    const completed = all.filter((q) => q.status === "done").length;
    return { total: all.length, inProgress, pending, blocked, completed };
  }, [quests]);

  const openModal = useCallback(() => setModalOpen(true), []);

  const viewFilters: { key: ViewFilter; label: string }[] = [
    { key: "active", label: "Active" },
    { key: "all", label: "All" },
    { key: "closed", label: "Closed" },
  ];

  return (
    <div className="page-content q-page">
      {/* Hero */}
      <div className="q-hero">
        <div className="q-hero-left">
          <h1 className="q-hero-title">Quests</h1>
          <p className="q-hero-subtitle">Track and manage work across all agents</p>
        </div>
        <button className="q-btn q-btn-primary" onClick={openModal}>
          New Quest
          <kbd className="q-kbd">C</kbd>
        </button>
      </div>

      {/* Stats */}
      <div className="q-stats">
        <div className="q-stat">
          <span className="q-stat-value">{stats.inProgress}</span>
          <span className="q-stat-label">In Progress</span>
        </div>
        <div className="q-stat-divider" />
        <div className="q-stat">
          <span className="q-stat-value">{stats.pending}</span>
          <span className="q-stat-label">Pending</span>
        </div>
        <div className="q-stat-divider" />
        <div className="q-stat">
          <span className="q-stat-value q-stat-warning">{stats.blocked}</span>
          <span className="q-stat-label">Blocked</span>
        </div>
        <div className="q-stat-divider" />
        <div className="q-stat">
          <span className="q-stat-value q-stat-success">{stats.completed}</span>
          <span className="q-stat-label">Completed</span>
        </div>
      </div>

      {/* Toolbar */}
      <div className="q-toolbar">
        <div className="q-filter-tabs">
          {viewFilters.map((f) => (
            <button
              key={f.key}
              className={`q-filter-tab${viewFilter === f.key ? " active" : ""}`}
              onClick={() => setViewFilter(f.key)}
              type="button"
            >
              {f.label}
              {f.key === "active" && totalActive > 0 && (
                <span className="q-filter-tab-count">{totalActive}</span>
              )}
            </button>
          ))}
        </div>

        <div className="q-toolbar-right">
          <div className="q-search-wrap">
            <svg className="q-search-icon" viewBox="0 0 16 16" fill="none">
              <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.5" />
              <path d="M10.5 10.5L14 14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
            <input
              className="q-search"
              placeholder="Filter..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>

          <select
            className="q-agent-filter"
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
        </div>
      </div>

      {/* List */}
      <div className="q-list">
        {groups.length === 0 && (
          <div className="q-empty">
            <span className="q-empty-text">No quests</span>
          </div>
        )}
        {groups.map((g) => (
          <QuestGroupSection
            key={g.status}
            group={g}
            defaultOpen={g.status !== "done" && g.status !== "cancelled"}
          />
        ))}
      </div>

      <CreateQuestModal open={modalOpen} onClose={() => setModalOpen(false)} />
    </div>
  );
}
