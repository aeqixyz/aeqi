import { useEffect, useState, useRef, useCallback } from "react";
import { useSearchParams } from "react-router-dom";
import Markdown from "react-markdown";
import { api } from "@/lib/api";
import StatusBadge from "@/components/StatusBadge";
import { useChatStore } from "@/store/chat";
import { useAuthStore } from "@/store/auth";

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <button className="session-msg-copy" onClick={handleCopy} title={copied ? "Copied" : "Copy"}>
      {copied ? (
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
          <path d="M3 8.5l3 3 7-7" />
        </svg>
      ) : (
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
          <rect x="5" y="5" width="9" height="9" rx="2" />
          <path d="M5 11H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h5a2 2 0 0 1 2 2v1" />
        </svg>
      )}
    </button>
  );
}

interface ToolEvent {
  type: "start" | "complete" | "turn" | "status";
  name: string;
  success?: boolean;
  input_preview?: string;
  output_preview?: string;
  duration_ms?: number;
  timestamp: number;
}

function ExpandableOutput({ text, limit = 100 }: { text: string; limit?: number }) {
  const [expanded, setExpanded] = useState(false);
  const needsExpand = text.length > limit;

  return (
    <div className="session-tool-output">
      {expanded || !needsExpand ? text : text.slice(0, limit) + "..."}
      {needsExpand && (
        <span
          className="session-tool-expand"
          onClick={(e) => { e.stopPropagation(); setExpanded(!expanded); }}
        >
          {expanded ? "show less" : "show more"}
        </span>
      )}
    </div>
  );
}

function ToolPanel({ events }: { events: ToolEvent[] }) {
  if (events.length === 0) return null;

  const completed = events.filter(e => e.type === "complete");
  const totalDuration = completed
    .filter(e => e.duration_ms)
    .reduce((sum, e) => sum + (e.duration_ms || 0), 0);

  return (
    <div className="session-tool-live">
      <div className="session-tool-live-header">
        {completed.length} tool {completed.length === 1 ? "call" : "calls"}
        {totalDuration > 0 && <span> · {formatMs(totalDuration)}</span>}
      </div>
      {events.filter(e => e.type === "complete" || e.type === "turn" || e.type === "status").map((ev, i) => (
        ev.type === "turn" ? (
          <div key={i} className="session-tool-live-item turn">
            <span className="session-tool-live-name" style={{ fontWeight: 600, opacity: 0.5, fontSize: "0.85em", letterSpacing: "0.03em" }}>{ev.name}</span>
          </div>
        ) : ev.type === "status" ? (
          <div key={i} className="session-tool-live-item status">
            <span className="session-tool-live-name" style={{ fontStyle: "italic", opacity: 0.6 }}>{ev.name}</span>
          </div>
        ) : (
          <div key={i} className={`session-tool-live-item complete`}>
            <span className="session-tool-live-icon">{ev.success ? "✓" : "✗"}</span>
            <span className="session-tool-live-name">{ev.name}</span>
            {ev.input_preview && (
              <span className="session-tool-input">{ev.input_preview}</span>
            )}
            {ev.duration_ms != null && (
              <span className="session-tool-ms">{formatMs(ev.duration_ms)}</span>
            )}
            {ev.output_preview && (
              <ExpandableOutput text={ev.output_preview} />
            )}
          </div>
        )
      ))}
    </div>
  );
}

const THINKING_WORDS = [
  "thinking", "reasoning", "analyzing", "considering", "processing",
  "pondering", "evaluating", "working", "exploring", "planning",
];

function ThinkingStatus({ toolName }: { toolName?: string }) {
  const [wordIdx, setWordIdx] = useState(() => Math.floor(Math.random() * THINKING_WORDS.length));

  useEffect(() => {
    if (toolName) return; // Don't rotate when showing tool name
    const interval = setInterval(() => {
      setWordIdx(prev => (prev + 1) % THINKING_WORDS.length);
    }, 2000);
    return () => clearInterval(interval);
  }, [toolName]);

  if (toolName) {
    return <div className="session-msg-thinking">using {toolName}...</div>;
  }
  return <div className="session-msg-thinking">{THINKING_WORDS[wordIdx]}...</div>;
}

function formatMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

function formatDuration(startMs: number, endMs: number): string {
  const diff = endMs - startMs;
  if (diff < 1000) return "<1s";
  if (diff < 60000) return `${Math.round(diff / 1000)}s`;
  return `${Math.floor(diff / 60000)}m ${Math.round((diff % 60000) / 1000)}s`;
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** An ordered segment in the assistant response timeline. */
type MessageSegment =
  | { kind: "text"; text: string }
  | { kind: "tool"; event: ToolEvent }
  | { kind: "status"; text: string };

interface Message {
  role: string;
  content: string;
  /** Ordered timeline of text, tool calls, and status events — renders interleaved. */
  segments?: MessageSegment[];
  timestamp?: number;
  duration?: string;
  toolEvents?: ToolEvent[];
  costUsd?: number;
  tokenUsage?: { prompt: number; completion: number };
  eventType?: string;
  taskId?: string;
}

interface SubagentInfo {
  workerName: string;
  subject: string;
  startTime: number;
  status: "running" | "completed" | "failed";
  outcome?: string;
  duration?: string;
}

interface SessionInfo {
  id: string;
  name: string;
  type: "perpetual" | "active" | "history";
  status?: string;
  agent?: string;
  skill?: string;
  time?: string;
  sessionId?: string;
  agentId?: string;
}

function timeAgo(ts: string): string {
  const diff = Date.now() - new Date(ts).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "now";
  if (mins < 60) return `${mins}m`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h`;
  return `${Math.floor(hrs / 24)}d`;
}

export default function SessionsPage() {
  const [searchParams] = useSearchParams();
  const agentFilter = searchParams.get("agent");
  const selectedAgent = useChatStore((s) => s.selectedAgent);
  const token = useAuthStore((s) => s.token);

  // Derive UUID for API calls and display name for UI
  const agentId = selectedAgent?.id;
  const agentName = agentFilter || selectedAgent?.name;

  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string>("perpetual");
  const [sessionCounter, setSessionCounter] = useState(0);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [streamText, setStreamText] = useState("");
  const [liveToolEvents, setLiveToolEvents] = useState<ToolEvent[]>([]);
  const [liveSubagents, setLiveSubagents] = useState<SubagentInfo[]>([]);
  // collapsedTools removed — tool panels are always expanded now
  const [thinkingStart, setThinkingStart] = useState<number | null>(null);
  const messagesEnd = useRef<HTMLDivElement>(null);
  const wsRef = useRef<WebSocket | null>(null);

  // Build session list — fetch real sessions from backend + tasks
  useEffect(() => {
    const list: SessionInfo[] = [];
    const perpetualName = agentName
      ? agentName.charAt(0).toUpperCase() + agentName.slice(1)
      : "Assistant";
    list.push({ id: "perpetual", name: perpetualName, type: "perpetual", status: "live" });

    // Fetch real sessions to get session_id and agent_id metadata
    const sessionsPromise = agentId
      ? api.getSessions(agentId).then((d: any) => {
          const sessions = d.sessions || [];
          // Find the active/permanent session for this agent
          const active = sessions.find((s: any) => s.status === "active");
          if (active) {
            list[0].sessionId = active.id;
            list[0].agentId = active.agent_id;
          }
          // Add other sessions as entries
          for (const s of sessions) {
            if (s.id === active?.id) continue;
            list.push({
              id: `session-${s.id}`,
              name: s.id.slice(0, 8),
              type: s.status === "active" ? "active" : "history",
              status: s.status,
              time: s.created_at,
              sessionId: s.id,
              agentId: s.agent_id,
            });
          }
        }).catch(() => {})
      : Promise.resolve();

    const tasksPromise = api.getTasks({}).then((d: any) => {
      const tasks = d.tasks || [];
      const filtered = agentName
        ? tasks.filter((t: any) =>
            (t.assignee || t.agent_id || "").toLowerCase().includes(agentName.toLowerCase())
          )
        : tasks;
      for (const t of filtered) {
        const isActive = t.status === "InProgress" || t.status === "in_progress";
        list.push({
          id: t.id,
          name: t.subject,
          type: isActive ? "active" : "history",
          status: t.status,
          skill: t.skill,
          time: t.created_at,
        });
      }
    }).catch(() => {});

    Promise.all([sessionsPromise, tasksPromise]).then(() => setSessions([...list]));
  }, [agentId, agentName]);

  const [childSessions, setChildSessions] = useState<SessionInfo[]>([]);
  const [linkedTasks, setLinkedTasks] = useState<any[]>([]);

  useEffect(() => {
    if (!agentName) return;
    api.getTasks({}).then((d: any) => {
      const tasks = (d.tasks || []).filter((t: any) => {
        const assignee = (t.assignee || "").toLowerCase();
        return assignee.includes(agentName.toLowerCase());
      });
      setLinkedTasks(tasks);
    }).catch(() => {});
  }, [agentName]);

  // Load child sessions for the selected session
  useEffect(() => {
    const sel = sessions.find(s => s.id === activeSessionId);
    if (sel?.sessionId) {
      api.getSessionChildren(sel.sessionId).then((d: any) => {
        const children = (d.sessions || []).map((s: any) => ({
          id: `child-${s.id}`,
          name: s.name || s.task_id || "subtask",
          type: s.status === "active" ? "active" as const : "history" as const,
          status: s.status,
          time: s.created_at,
          sessionId: s.id,
          agentId: s.agent_id,
        }));
        setChildSessions(children);
      }).catch(() => setChildSessions([]));
    } else {
      setChildSessions([]);
    }
  }, [activeSessionId, sessions]);

  // Reconstruct interleaved segments from a flat timeline (messages + tool events).
  // DB order is: user, tool_complete(s), assistant — tool events recorded during streaming
  // come before the final assistant text. We collect pending tool events and attach them
  // to the next assistant message.
  const processRawMessages = useCallback((rawMessages: any[]): Message[] => {
    const processed: Message[] = [];
    let pendingToolSegments: MessageSegment[] = [];

    for (const m of rawMessages) {
      const eventType = m.event_type || "message";
      if (eventType === "tool_complete") {
        // Collect tool events — they will attach to the next assistant message
        const meta = m.metadata || {};
        pendingToolSegments.push({
          kind: "tool" as const,
          event: {
            type: "complete" as const,
            name: meta.tool_name || m.content || "tool",
            success: meta.success !== false,
            input_preview: meta.input_preview,
            output_preview: meta.output_preview,
            duration_ms: meta.duration_ms,
            timestamp: m.created_at ? new Date(m.created_at).getTime() : Date.now(),
          },
        });
      } else if (m.role === "assistant") {
        // Build segments: pending tools + final text
        const segments: MessageSegment[] = [
          ...pendingToolSegments,
          { kind: "text" as const, text: m.content },
        ];
        pendingToolSegments = [];
        processed.push({
          ...m,
          segments,
          timestamp: m.created_at ? new Date(m.created_at).getTime() : undefined,
        });
      } else {
        // Non-assistant, non-tool: flush any orphaned pending tools (shouldn't happen normally)
        pendingToolSegments = [];
        processed.push({
          ...m,
          timestamp: m.created_at ? new Date(m.created_at).getTime() : (m.timestamp ? new Date(m.timestamp).getTime() : undefined),
        });
      }
    }
    return processed;
  }, []);

  // Load messages for selected session
  useEffect(() => {
    if (activeSessionId === "perpetual" && (agentId || agentName)) {
      const params: { agent_id?: string; channel_name?: string; limit: number } = { limit: 50 };
      if (agentId) {
        params.agent_id = agentId;
      } else if (agentName) {
        params.channel_name = agentName.toLowerCase();
      }
      api.getSessionMessages(params)
        .then((d: any) => setMessages(processRawMessages(d.messages || [])))
        .catch(() => setMessages([]));
    } else if (activeSessionId === "perpetual") {
      api.getSessionMessages({ limit: 50 })
        .then((d: any) => setMessages(processRawMessages(d.messages || [])))
        .catch(() => setMessages([]));
    } else if (activeSessionId.startsWith("new-")) {
      setMessages([]);
    } else {
      api.getSessionMessages({ channel_name: `transcript:task:${activeSessionId}`, limit: 50 })
        .then((d: any) => setMessages(processRawMessages(d.messages || [])))
        .catch(() => setMessages([]));
    }

    // Also fetch audit for task events
    if (agentName) {
      api.getAudit({ last: 50 }).then((d: any) => {
        const entries = (d.entries || d.audit || []).filter(
          (e: any) => e.agent && e.agent.toLowerCase().includes(agentName.toLowerCase()) && e.task_id
        );
        if (entries.length > 0) {
          setMessages((prev) => {
            const taskEvents = entries.map((e: any) => ({
              role: "task_event",
              content: e.summary || `Task ${e.task_id} — ${e.decision_type}`,
              timestamp: new Date(e.timestamp).getTime(),
              eventType: e.decision_type,
              taskId: e.task_id,
            }));
            const merged = [...prev, ...taskEvents].sort((a, b) => (a.timestamp || 0) - (b.timestamp || 0));
            return merged;
          });
        }
      }).catch(() => {});
    }
  }, [activeSessionId, agentId, agentName]);

  useEffect(() => {
    messagesEnd.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamText]);

  const handleSend = useCallback(() => {
    if (!input.trim() || streaming || !token) return;

    // Capture start time as local variable — avoids stale closure on thinkingStart state.
    const startTime = Date.now();
    const userMsg: Message = { role: "user", content: input, timestamp: startTime };
    setMessages((prev) => [...prev, userMsg]);
    setInput("");
    setStreaming(true);
    setStreamText("");
    setLiveToolEvents([]);
    setLiveSubagents([]);
    setThinkingStart(startTime);

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(
      `${protocol}//${window.location.host}/api/chat/stream?token=${token}`
    );
    wsRef.current = ws;

    ws.onopen = () => {
      ws.send(JSON.stringify({ message: userMsg.content, agent_id: agentId || undefined }));
    };

    let fullText = "";
    let done = false;
    let lastToolName: string | undefined;
    const toolEvents: ToolEvent[] = [];
    const segments: MessageSegment[] = [];

    // Helper: append text to the last text segment, or create a new one.
    const appendText = (delta: string) => {
      const last = segments[segments.length - 1];
      if (last && last.kind === "text") {
        last.text += delta;
      } else {
        segments.push({ kind: "text", text: delta });
      }
      fullText += delta;
    };

    ws.onmessage = (e) => {
      try {
        const event = JSON.parse(e.data);
        switch (event.type) {
          case "TextDelta": {
            appendText(event.text || event.delta || "");
            setStreamText(fullText);
            break;
          }
          case "ToolCall":
          case "ToolStart": {
            const name = event.name || event.tool_name || event.tool_use_id || "tool";
            const ev: ToolEvent = { type: "start", name, timestamp: Date.now() };
            toolEvents.push(ev);
            segments.push({ kind: "tool", event: ev });
            lastToolName = name;
            setLiveToolEvents([...toolEvents]);
            break;
          }
          case "ToolResult":
          case "ToolComplete": {
            const name = event.name || event.tool_name || event.tool_use_id || "tool";
            const completed: ToolEvent = {
              type: "complete",
              name,
              success: event.success !== false,
              input_preview: event.input_preview || undefined,
              output_preview: event.output_preview || event.output || "",
              duration_ms: event.duration_ms,
              timestamp: Date.now(),
            };
            // Replace start → complete in toolEvents
            const startIdx = toolEvents.findIndex(e => e.type === "start" && e.name === name);
            if (startIdx >= 0) toolEvents[startIdx] = completed;
            else toolEvents.push(completed);
            // Replace in segments too
            const segIdx = segments.findIndex(s => s.kind === "tool" && s.event.type === "start" && s.event.name === name);
            if (segIdx >= 0) segments[segIdx] = { kind: "tool", event: completed };
            else segments.push({ kind: "tool", event: completed });
            lastToolName = undefined;
            setLiveToolEvents([...toolEvents]);
            break;
          }
          case "TurnStart": {
            const turnNum = event.turn || 0;
            const ev: ToolEvent = { type: "turn", name: `Turn ${turnNum}`, timestamp: Date.now() };
            toolEvents.push(ev);
            segments.push({ kind: "status", text: `Turn ${turnNum}` });
            setLiveToolEvents([...toolEvents]);
            break;
          }
          case "TurnComplete": {
            // Track per-turn token usage (no UI action needed)
            break;
          }
          case "Status": {
            const statusMsg = event.message || "";
            toolEvents.push({ type: "status", name: statusMsg, timestamp: Date.now() });
            segments.push({ kind: "status", text: statusMsg });
            setLiveToolEvents([...toolEvents]);
            break;
          }
          case "Compacted": {
            toolEvents.push({ type: "status", name: `Context compacted (${event.original_messages}\u2192${event.remaining_messages} msgs)`, timestamp: Date.now() });
            setLiveToolEvents([...toolEvents]);
            break;
          }
          case "MemoryActivity": {
            const desc = `${event.action}: ${event.key}`;
            toolEvents.push({ type: "status", name: desc, timestamp: Date.now() });
            setLiveToolEvents([...toolEvents]);
            break;
          }
          case "DelegateStart": {
            const workerName = event.worker_name || "subagent";
            const subject = event.task_subject || "delegated task";
            toolEvents.push({ type: "start", name: `delegate: ${workerName}`, timestamp: Date.now() });
            segments.push({ kind: "status", text: `Delegating to ${workerName}: ${subject}` });
            setLiveToolEvents([...toolEvents]);
            setLiveSubagents(prev => [...prev, {
              workerName,
              subject,
              startTime: Date.now(),
              status: "running",
            }]);
            break;
          }
          case "DelegateComplete": {
            const doneWorker = event.worker_name || "subagent";
            const delegateStartIdx = toolEvents.findIndex(e => e.type === "start" && e.name === `delegate: ${doneWorker}`);
            if (delegateStartIdx >= 0) {
              toolEvents[delegateStartIdx] = { type: "complete", name: `delegate: ${doneWorker}`, success: true, output_preview: event.outcome, timestamp: Date.now() };
            }
            const outcomePreview = (event.outcome || "").slice(0, 200);
            segments.push({ kind: "status", text: `${doneWorker} completed: ${outcomePreview}` });
            setLiveToolEvents([...toolEvents]);
            setLiveSubagents(prev => prev.map(s =>
              s.workerName === doneWorker && s.status === "running"
                ? { ...s, status: "completed" as const, outcome: event.outcome, duration: formatDuration(s.startTime, Date.now()) }
                : s
            ));
            break;
          }
          case "Complete":
          case "done": {
            // The agent emits a Complete event (no "done" field) which the daemon
            // forwards, then the daemon sends its own summary with done:true.
            // Only finalize on the done:true summary (or standalone Complete with
            // done:true) to avoid double-finalization.
            if (!event.done && event.type === "Complete") {
              // Agent Complete event — capture tokens but wait for daemon done.
              // Tokens from the agent event use total_* naming.
              break;
            }
            done = true;
            const endTime = Date.now();
            const duration = formatDuration(startTime, endTime);
            // Always add the message — even with no text, show tool events + duration.
            const hasContent = fullText || (toolEvents.length > 0);
            if (hasContent) {
              const promptTok = event.prompt_tokens || 0;
              const completionTok = event.completion_tokens || 0;
              setMessages((prev) => [...prev, {
                role: "assistant",
                content: fullText || "(no text output)",
                segments: segments.length > 0 ? [...segments] : undefined,
                timestamp: endTime,
                duration,
                toolEvents: toolEvents.length > 0 ? [...toolEvents] : undefined,
                costUsd: event.cost_usd || undefined,
                tokenUsage: (promptTok || completionTok) ? { prompt: promptTok, completion: completionTok } : undefined,
              }]);
            }
            setStreamText("");
            setStreaming(false);
            setLiveToolEvents([]);
            setThinkingStart(null);
            ws.close();
            break;
          }
          case "Error":
            done = true;
            setMessages((prev) => [...prev, {
              role: "error",
              content: event.message || "Unknown error",
              timestamp: Date.now(),
              duration: formatDuration(startTime, Date.now()),
            }]);
            setStreaming(false);
            setThinkingStart(null);
            ws.close();
            break;
        }
      } catch {}
    };

    ws.onerror = () => { setStreaming(false); setThinkingStart(null); };
    ws.onclose = () => {
      // Fallback: if WS closed before Complete event, finalize the message.
      if (!done && fullText) {
        const endTime = Date.now();
        setMessages((prev) => [...prev, {
          role: "assistant",
          content: fullText,
          timestamp: endTime,
          duration: formatDuration(startTime, endTime),
          toolEvents: toolEvents.length > 0 ? [...toolEvents] : undefined,
        }]);
        setStreamText("");
      }
      setStreaming(false);
      setThinkingStart(null);
    };
  }, [input, streaming, token, agentId]);

  if (!agentName && !agentId) {
    return (
      <div className="sessions-page">
        <div className="sessions-empty">Select an agent to view sessions</div>
      </div>
    );
  }

  const activeSessions = sessions.filter((s) => s.type === "active");
  const closedSessions = sessions.filter((s) => s.type === "history");

  return (
    <div className="sessions-split">
      <div className="sessions-list-pane">
        <div className="sessions-list-title">Sessions</div>
        <div className="session-list-add" onClick={() => {
          const id = `new-${Date.now()}`;
          const num = sessionCounter + 1;
          setSessionCounter(num);
          setSessions((prev) => [...prev, {
            id, name: `session ${num}`, type: "active" as const, status: "new",
          }]);
          setActiveSessionId(id);
          setMessages([]);
          setStreamText("");
        }}>+</div>

        <div className="sessions-list-section">
          <div className="sessions-list-header">permanent</div>
          <div
            className={`session-list-item${activeSessionId === "perpetual" ? " active" : ""}`}
            onClick={() => setActiveSessionId("perpetual")}
          >
            <span className="session-list-dot">●</span>
            <span className="session-list-name">{agentName || "Agent"}</span>
          </div>
        </div>

        {liveSubagents.length > 0 && (
          <div className="sessions-list-section">
            <div className="sessions-list-header">active work</div>
            {liveSubagents.map((s, i) => (
              <div key={i} className={`session-list-item subagent-item ${s.status}`} title={s.subject}>
                <span className="session-list-dot subagent-dot">
                  {s.status === "running" ? "\u27F3" : s.status === "completed" ? "\u2713" : "\u2717"}
                </span>
                <span className="session-list-name">{s.workerName}</span>
                {s.status === "running" && (
                  <span className="session-list-time"><SubagentTimer start={s.startTime} /></span>
                )}
                {s.duration && <span className="session-list-time">{s.duration}</span>}
              </div>
            ))}
          </div>
        )}

        {childSessions.length > 0 && (
          <div className="sessions-list-section">
            <div className="sessions-list-header">spawned work</div>
            {childSessions.map(s => (
              <div key={s.id}
                className={`session-list-item${activeSessionId === s.id ? " active" : ""}`}
                onClick={() => {
                  setActiveSessionId(s.id);
                  if (s.sessionId) {
                    api.getSessionMessages({ session_id: s.sessionId, limit: 50 })
                      .then((d: any) => setMessages(
                        (d.messages || []).map((m: any) => ({
                          ...m,
                          timestamp: m.created_at ? new Date(m.created_at).getTime() : undefined,
                        }))
                      ))
                      .catch(() => setMessages([]));
                  }
                }}
              >
                <span className="session-list-dot">{s.status === "active" ? "\u25CF" : "\u25CB"}</span>
                <span className="session-list-name">{s.name}</span>
                {s.time && <span className="session-list-time">{timeAgo(s.time)}</span>}
              </div>
            ))}
          </div>
        )}

        {activeSessions.length > 0 && (
          <div className="sessions-list-section">
            <div className="sessions-list-header">active</div>
            {activeSessions.map((s) => (
              <div key={s.id}
                className={`session-list-item${activeSessionId === s.id ? " active" : ""}`}
                onClick={() => setActiveSessionId(s.id)}
              >
                <span className="session-list-dot">●</span>
                <span className="session-list-name">{s.name}</span>
                {s.status && s.type !== "perpetual" && (
                  <StatusBadge status={s.status} size="sm" />
                )}
                {s.time && <span className="session-list-time">{timeAgo(s.time)}</span>}
              </div>
            ))}
          </div>
        )}

        {linkedTasks.length > 0 && (
          <div className="sessions-list-section">
            <div className="sessions-list-header">linked tasks</div>
            {linkedTasks.slice(0, 10).map((t: any) => (
              <div key={t.id}
                className={`session-list-item${activeSessionId === t.id ? " active" : ""}`}
                onClick={() => setActiveSessionId(t.id)}
              >
                <StatusBadge status={t.status} size="sm" />
                <span className="session-list-name">{t.subject}</span>
              </div>
            ))}
          </div>
        )}

        {closedSessions.length > 0 && (
          <div className="sessions-list-section">
            <div className="sessions-list-header">closed</div>
            {closedSessions.map((s) => (
              <div key={s.id}
                className={`session-list-item${activeSessionId === s.id ? " active" : ""}`}
                onClick={() => setActiveSessionId(s.id)}
              >
                <span className="session-list-dot dim">○</span>
                <span className="session-list-name">{s.name}</span>
                {s.status && s.type !== "perpetual" && (
                  <StatusBadge status={s.status} size="sm" />
                )}
                {s.time && <span className="session-list-time">{timeAgo(s.time)}</span>}
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="sessions-transcript-pane">
        {(() => {
          const sel = sessions.find((s) => s.id === activeSessionId);
          const displayName = selectedAgent?.display_name || selectedAgent?.name || agentName || "Agent";
          const modelName = selectedAgent?.model;
          const sessionStatus = sel?.status || (sel?.type === "perpetual" ? "live" : undefined);
          return (
            <div className="session-header">
              <span className="session-header-name">{displayName}</span>
              {modelName && <span className="session-header-model">{modelName}</span>}
              {sessionStatus && (
                <span className={`session-header-status ${sessionStatus === "live" || sessionStatus === "active" ? "active" : ""}`}>
                  {sessionStatus}
                </span>
              )}
              {sel?.sessionId && (
                <span className="session-header-id">{sel.sessionId.slice(0, 8)}</span>
              )}
            </div>
          );
        })()}
        <div className="session-messages">
          {messages.map((msg, i) => {
            if (msg.role === "task_event") {
              return (
                <div key={i} className={`session-msg session-msg-task-event session-task-event-${(msg.eventType || "").includes("create") ? "created" : (msg.eventType || "").includes("complete") || (msg.eventType || "").includes("close") ? "completed" : (msg.eventType || "").includes("block") ? "blocked" : "started"}`}>
                  <div className="session-task-event">
                    <span className="session-task-event-icon">
                      {(msg.eventType || "").includes("create") ? "+" : (msg.eventType || "").includes("complete") || (msg.eventType || "").includes("close") ? "\u2713" : (msg.eventType || "").includes("block") ? "!" : "\u2192"}
                    </span>
                    <span className="session-task-event-text">{msg.content}</span>
                    {msg.timestamp && <span className="session-task-event-time">{formatTime(msg.timestamp)}</span>}
                  </div>
                </div>
              );
            }
            return (
            <div key={i} className={`session-msg session-msg-${msg.role}`}>
              <div className="session-msg-header">
                <span className="session-msg-role">{msg.role}</span>
                {msg.timestamp && (
                  <span className="session-msg-time">{formatTime(msg.timestamp)}</span>
                )}
                {msg.duration && (
                  <span className="session-msg-duration">{msg.duration}</span>
                )}
                {msg.costUsd != null && msg.costUsd > 0 && (
                  <span className="session-msg-cost">${msg.costUsd.toFixed(4)}</span>
                )}
                {msg.tokenUsage && (msg.tokenUsage.prompt > 0 || msg.tokenUsage.completion > 0) && (
                  <span className="session-msg-tokens">{msg.tokenUsage.prompt}\u2192{msg.tokenUsage.completion} tok</span>
                )}
              </div>

              {/* Render interleaved segments if available, else fallback */}
              {msg.segments && msg.segments.length > 0 ? (
                <>
                  {msg.segments.map((seg, si) =>
                    seg.kind === "text" ? (
                      <div key={si} className="session-msg-content">
                        <Markdown>{seg.text}</Markdown>
                      </div>
                    ) : seg.kind === "tool" ? (
                      <div key={si} className="session-tool-inline">
                        <span className="session-tool-live-icon">
                          {seg.event.type === "start" ? "⟳" : seg.event.success ? "✓" : "✗"}
                        </span>
                        <span className="session-tool-live-name">{seg.event.name}</span>
                        {seg.event.input_preview && (
                          <span className="session-tool-input">{seg.event.input_preview}</span>
                        )}
                        {seg.event.duration_ms != null && (
                          <span className="session-tool-ms">{formatMs(seg.event.duration_ms)}</span>
                        )}
                        {seg.event.output_preview && (
                          <ExpandableOutput text={seg.event.output_preview} />
                        )}
                      </div>
                    ) : seg.kind === "status" ? (
                      <div key={si} className="session-status-item">{seg.text}</div>
                    ) : null
                  )}
                  {msg.role === "assistant" && <CopyButton text={msg.content} />}
                </>
              ) : (
                <>
                  {msg.toolEvents && msg.toolEvents.length > 0 && (
                    <ToolPanel events={msg.toolEvents} />
                  )}
                  <div className="session-msg-content">
                    {msg.role === "assistant" ? (
                      <Markdown>{msg.content}</Markdown>
                    ) : (
                      <span>{msg.content}</span>
                    )}
                  </div>
                  {msg.role === "assistant" && <CopyButton text={msg.content} />}
                </>
              )}
            </div>
            );
          })}

          {/* Live activity while streaming */}
          {streaming && (
            <div className="session-msg session-msg-assistant session-msg-streaming">
              <div className="session-msg-header">
                <span className="session-msg-role">assistant</span>
                {thinkingStart && (
                  <ThinkingTimer start={thinkingStart} />
                )}
              </div>

              {/* Streaming text */}
              {streamText && (
                <div className="session-msg-content">
                  <Markdown>{streamText}</Markdown>
                </div>
              )}

              {/* Live tool events — always visible below text */}
              {liveToolEvents.length > 0 && (
                <div className="session-tool-live">
                  <div className="session-tool-live-header">
                    {liveToolEvents.some(e => e.type === "start")
                      ? "working..."
                      : `${liveToolEvents.filter(e => e.type === "complete").length} tool calls`
                    }
                  </div>
                  {liveToolEvents.map((ev, i) => (
                    ev.type === "turn" ? (
                      <div key={i} className="session-tool-live-item turn">
                        <span className="session-tool-live-name" style={{ fontWeight: 600, opacity: 0.5, fontSize: "0.85em", letterSpacing: "0.03em" }}>{ev.name}</span>
                      </div>
                    ) : ev.type === "status" ? (
                      <div key={i} className="session-tool-live-item status">
                        <span className="session-tool-live-name" style={{ fontStyle: "italic", opacity: 0.6 }}>{ev.name}</span>
                      </div>
                    ) : (
                      <div key={i} className={`session-tool-live-item ${ev.type}`}>
                        <span className="session-tool-live-icon">
                          {ev.type === "start" ? "⟳" : ev.success ? "✓" : "✗"}
                        </span>
                        <span className="session-tool-live-name">{ev.name}</span>
                        {ev.duration_ms != null && (
                          <span className="session-tool-ms">{formatMs(ev.duration_ms)}</span>
                        )}
                        {ev.type === "complete" && ev.output_preview && (
                          <ExpandableOutput text={ev.output_preview} />
                        )}
                      </div>
                    )
                  ))}
                </div>
              )}

              {/* Dynamic thinking status — shows when no text and no tools yet */}
              {!streamText && !liveToolEvents.length && (
                <ThinkingStatus />
              )}
              {/* Show active tool name — visible even while text is streaming */}
              {liveToolEvents.some(e => e.type === "start") && (
                <ThinkingStatus toolName={liveToolEvents.filter(e => e.type === "start").pop()?.name} />
              )}
            </div>
          )}

          <div ref={messagesEnd} />
        </div>

        <div className="session-input-wrap">
          <input
            className="session-input"
            type="text"
            placeholder={streaming ? "Responding..." : `Message ${agentName || "agent"}...`}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && handleSend()}
            disabled={streaming}
          />
        </div>
      </div>
    </div>
  );
}

function ThinkingTimer({ start }: { start: number }) {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setElapsed(Date.now() - start);
    }, 100);
    return () => clearInterval(interval);
  }, [start]);

  return <span className="session-msg-duration">{formatDuration(start, start + elapsed)}</span>;
}

function SubagentTimer({ start }: { start: number }) {
  const [, setTick] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => setTick(t => t + 1), 1000);
    return () => clearInterval(interval);
  }, [start]);

  return <>{formatDuration(start, Date.now())}</>;
}
