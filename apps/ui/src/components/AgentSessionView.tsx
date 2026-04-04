import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import Markdown from "react-markdown";
import { api } from "@/lib/api";
import { useChatStore } from "@/store/chat";
import { useAuthStore } from "@/store/auth";
import { useDaemonStore } from "@/store/daemon";

// ── Types ──

interface ToolEvent {
  type: "start" | "complete" | "turn" | "status";
  name: string;
  success?: boolean;
  input_preview?: string;
  output_preview?: string;
  duration_ms?: number;
  timestamp: number;
}

type MessageSegment =
  | { kind: "text"; text: string }
  | { kind: "tool"; event: ToolEvent }
  | { kind: "status"; text: string };

interface Message {
  role: string;
  content: string;
  segments?: MessageSegment[];
  timestamp?: number;
  duration?: string;
  toolEvents?: ToolEvent[];
  costUsd?: number;
  tokenUsage?: { prompt: number; completion: number };
  eventType?: string;
  taskId?: string;
}

// ── Helpers ──

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
  return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

// ── Sub-components ──

function ExpandableOutput({ text, limit = 100 }: { text: string; limit?: number }) {
  const [expanded, setExpanded] = useState(false);
  const needsExpand = text.length > limit;
  return (
    <div className="session-tool-output">
      {expanded || !needsExpand ? text : text.slice(0, limit) + "..."}
      {needsExpand && (
        <span className="session-tool-expand" onClick={(e) => { e.stopPropagation(); setExpanded(!expanded); }}>
          {expanded ? "show less" : "show more"}
        </span>
      )}
    </div>
  );
}

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

const THINKING_WORDS = [
  "thinking", "reasoning", "analyzing", "considering", "processing",
  "pondering", "evaluating", "working", "exploring", "planning",
];

function ThinkingStatus({ toolName }: { toolName?: string }) {
  const [wordIdx, setWordIdx] = useState(() => Math.floor(Math.random() * THINKING_WORDS.length));
  useEffect(() => {
    if (toolName) return;
    const interval = setInterval(() => setWordIdx(prev => (prev + 1) % THINKING_WORDS.length), 2000);
    return () => clearInterval(interval);
  }, [toolName]);
  if (toolName) return <div className="session-msg-thinking">using {toolName}...</div>;
  return <div className="session-msg-thinking">{THINKING_WORDS[wordIdx]}...</div>;
}

function ThinkingTimer({ start }: { start: number }) {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    const interval = setInterval(() => setElapsed(Date.now() - start), 100);
    return () => clearInterval(interval);
  }, [start]);
  return <span className="session-msg-duration">{formatDuration(start, start + elapsed)}</span>;
}

// ── Main Component ──

interface SessionInfo {
  id: string;
  agent_id?: string;
  agent_name?: string;
  status: string;
  created_at: string;
  last_active?: string;
  message_count?: number;
  first_message?: string;
}

export default function AgentSessionView() {
  const selectedAgent = useChatStore((s) => s.selectedAgent);
  const token = useAuthStore((s) => s.token);
  const wsConnected = useDaemonStore((s) => s.wsConnected);

  const agentId = selectedAgent?.id;
  const agentName = selectedAgent?.name;
  const displayName = selectedAgent?.display_name || agentName || "Agent";

  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [showSessionList, setShowSessionList] = useState(false);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [streamText, setStreamText] = useState("");
  const [liveToolEvents, setLiveToolEvents] = useState<ToolEvent[]>([]);
  const [thinkingStart, setThinkingStart] = useState<number | null>(null);
  const messagesEnd = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const wsRef = useRef<WebSocket | null>(null);

  // Load sessions for this agent
  useEffect(() => {
    if (!agentId && !agentName) return;
    const id = agentId || agentName || "";
    api.getSessions(id)
      .then((d: any) => {
        const list: SessionInfo[] = d.sessions || [];
        setSessions(list);
        // Auto-select the most recent session
        if (list.length > 0 && !activeSessionId) {
          setActiveSessionId(list[0].id);
        }
      })
      .catch(() => setSessions([]));
  }, [agentId, agentName]);

  // Create a new session
  const handleNewSession = useCallback(() => {
    const id = agentId || agentName || "";
    if (!id) return;
    api.createSession(id)
      .then((d: any) => {
        if (d.session_id) {
          const newSession: SessionInfo = {
            id: d.session_id,
            agent_id: agentId || undefined,
            agent_name: agentName || undefined,
            status: "active",
            created_at: new Date().toISOString(),
          };
          setSessions((prev) => [newSession, ...prev]);
          setActiveSessionId(d.session_id);
          setMessages([]);
          setStreamText("");
          setShowSessionList(false);
        }
      })
      .catch(() => {});
  }, [agentId, agentName]);

  // Switch sessions
  const handleSelectSession = useCallback((sessionId: string) => {
    setActiveSessionId(sessionId);
    setMessages([]);
    setStreamText("");
    setLiveToolEvents([]);
    setShowSessionList(false);
  }, []);

  // Process raw messages from API into our format
  const processRawMessages = useCallback((rawMessages: any[]): Message[] => {
    const processed: Message[] = [];
    let pendingToolSegments: MessageSegment[] = [];

    for (const m of rawMessages) {
      const eventType = m.event_type || "message";
      if (eventType === "tool_complete") {
        const meta = m.metadata || {};
        pendingToolSegments.push({
          kind: "tool",
          event: {
            type: "complete",
            name: meta.tool_name || m.content || "tool",
            success: meta.success !== false,
            input_preview: meta.input_preview,
            output_preview: meta.output_preview,
            duration_ms: meta.duration_ms,
            timestamp: m.created_at ? new Date(m.created_at).getTime() : Date.now(),
          },
        });
      } else if (m.role === "assistant") {
        const segments: MessageSegment[] = [
          ...pendingToolSegments,
          { kind: "text", text: m.content },
        ];
        pendingToolSegments = [];
        processed.push({
          ...m,
          segments,
          timestamp: m.created_at ? new Date(m.created_at).getTime() : undefined,
        });
      } else {
        pendingToolSegments = [];
        processed.push({
          ...m,
          timestamp: m.created_at ? new Date(m.created_at).getTime() : (m.timestamp ? new Date(m.timestamp).getTime() : undefined),
        });
      }
    }
    return processed;
  }, []);

  // Load messages when session changes
  useEffect(() => {
    if (!activeSessionId && !agentId && !agentName) return;
    setMessages([]);
    setStreamText("");
    setLiveToolEvents([]);

    const params: { session_id?: string; agent_id?: string; channel_name?: string; limit: number } = { limit: 50 };
    if (activeSessionId) {
      params.session_id = activeSessionId;
    } else if (agentId) {
      params.agent_id = agentId;
    } else if (agentName) {
      params.channel_name = agentName.toLowerCase();
    }

    api.getSessionMessages(params)
      .then((d: any) => setMessages(processRawMessages(d.messages || [])))
      .catch(() => setMessages([]));
  }, [activeSessionId, agentId, agentName, processRawMessages]);

  // Auto-scroll
  useEffect(() => {
    messagesEnd.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamText]);

  // Focus input on mount
  useEffect(() => {
    inputRef.current?.focus();
  }, [agentId]);

  // Send message via WebSocket streaming
  const handleSend = useCallback(() => {
    if (!input.trim() || streaming || !token) return;

    const startTime = Date.now();
    const userMsg: Message = { role: "user", content: input, timestamp: startTime };
    setMessages((prev) => [...prev, userMsg]);
    setInput("");
    setStreaming(true);
    setStreamText("");
    setLiveToolEvents([]);
    setThinkingStart(startTime);

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(
      `${protocol}//${window.location.host}/api/chat/stream?token=${token}`
    );
    wsRef.current = ws;

    ws.onopen = () => {
      ws.send(JSON.stringify({
        message: userMsg.content,
        agent_id: agentId || undefined,
        session_id: activeSessionId || undefined,
      }));
    };

    let fullText = "";
    let done = false;
    const toolEvents: ToolEvent[] = [];
    const segments: MessageSegment[] = [];

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
            const startIdx = toolEvents.findIndex(e => e.type === "start" && e.name === name);
            if (startIdx >= 0) toolEvents[startIdx] = completed;
            else toolEvents.push(completed);
            const segIdx = segments.findIndex(s => s.kind === "tool" && s.event.type === "start" && s.event.name === name);
            if (segIdx >= 0) segments[segIdx] = { kind: "tool", event: completed };
            else segments.push({ kind: "tool", event: completed });
            setLiveToolEvents([...toolEvents]);
            break;
          }
          case "TurnStart": {
            const turnNum = event.turn || 0;
            toolEvents.push({ type: "turn", name: `Turn ${turnNum}`, timestamp: Date.now() });
            segments.push({ kind: "status", text: `Turn ${turnNum}` });
            setLiveToolEvents([...toolEvents]);
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
            break;
          }
          case "Complete":
          case "done": {
            if (!event.done && event.type === "Complete") break;
            done = true;
            const endTime = Date.now();
            const duration = formatDuration(startTime, endTime);
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
      } catch { /* ignore malformed */ }
    };

    ws.onerror = () => { setStreaming(false); setThinkingStart(null); };
    ws.onclose = () => {
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

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    const el = e.target;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
  };

  if (!selectedAgent) return null;

  return (
    <div className="asv">
      {/* Session header */}
      <div className="asv-header">
        <div className="asv-header-info">
          <span className="asv-header-name">{displayName}</span>
          {selectedAgent.model && <span className="asv-header-model">{selectedAgent.model}</span>}
          <span className={`asv-header-dot ${wsConnected ? "live" : ""}`} />
        </div>
        <div className="asv-header-actions">
          <button
            className="asv-session-toggle"
            onClick={() => setShowSessionList(!showSessionList)}
            title="Sessions"
          >
            {sessions.length > 0 ? `${sessions.length} sessions` : "sessions"}
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
              <path d={showSessionList ? "M3 7.5L6 4.5L9 7.5" : "M3 4.5L6 7.5L9 4.5"} />
            </svg>
          </button>
          <button className="asv-new-session" onClick={handleNewSession} title="New conversation">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
              <path d="M7 3v8M3 7h8" />
            </svg>
          </button>
        </div>
      </div>

      {/* Session list dropdown */}
      {showSessionList && (
        <div className="asv-session-list">
          {sessions.length === 0 ? (
            <div className="asv-session-empty">No sessions yet. Start a conversation below.</div>
          ) : (
            sessions.map((s) => (
              <div
                key={s.id}
                className={`asv-session-item${s.id === activeSessionId ? " active" : ""}`}
                onClick={() => handleSelectSession(s.id)}
              >
                <div className="asv-session-item-info">
                  <span className="asv-session-item-preview">
                    {s.first_message || `Session ${s.id.slice(0, 8)}`}
                  </span>
                  <span className="asv-session-item-date">
                    {new Date(s.created_at).toLocaleDateString([], { month: "short", day: "numeric" })}
                  </span>
                </div>
                <div className="asv-session-item-meta">
                  <span className={`asv-session-status ${s.status}`}>{s.status}</span>
                  {s.message_count != null && (
                    <span className="asv-session-item-count">{s.message_count} msgs</span>
                  )}
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {/* Message transcript */}
      <div className="asv-messages">
        {messages.length === 0 && !streaming && (
          <div className="asv-empty">
            <div className="asv-empty-icon">
              <svg width="24" height="24" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.1" strokeLinecap="round" strokeLinejoin="round">
                <path d="M3 5l5-3 5 3" />
                <path d="M3 5v6l5 3 5-3V5" />
                <path d="M8 8v6" />
                <circle cx="8" cy="8" r="1.5" fill="currentColor" stroke="none" />
              </svg>
            </div>
            <div className="asv-empty-title">Message {displayName}</div>
            <div className="asv-empty-hint">Start a conversation with this agent.</div>
          </div>
        )}

        {messages.map((msg, i) => {
          if (msg.role === "task_event") {
            return (
              <div key={i} className="asv-task-event">
                <span className="asv-task-event-icon">
                  {(msg.eventType || "").includes("create") ? "+" : (msg.eventType || "").includes("complete") || (msg.eventType || "").includes("close") ? "\u2713" : (msg.eventType || "").includes("block") ? "!" : "\u2192"}
                </span>
                <span className="asv-task-event-text">{msg.content}</span>
                {msg.timestamp && <span className="asv-task-event-time">{formatTime(msg.timestamp)}</span>}
              </div>
            );
          }
          if (msg.role === "error") {
            return (
              <div key={i} className="asv-msg asv-msg-error">
                <div className="asv-msg-header">
                  <span className="asv-msg-role">error</span>
                  {msg.duration && <span className="asv-msg-duration">{msg.duration}</span>}
                </div>
                <div className="asv-msg-content">{msg.content}</div>
              </div>
            );
          }
          return (
            <div key={i} className={`asv-msg asv-msg-${msg.role}`}>
              <div className="asv-msg-header">
                <span className="asv-msg-role">{msg.role}</span>
                {msg.timestamp && <span className="asv-msg-time">{formatTime(msg.timestamp)}</span>}
                {msg.duration && <span className="asv-msg-duration">{msg.duration}</span>}
                {msg.costUsd != null && msg.costUsd > 0 && (
                  <span className="asv-msg-cost">${msg.costUsd.toFixed(4)}</span>
                )}
                {msg.tokenUsage && (msg.tokenUsage.prompt > 0 || msg.tokenUsage.completion > 0) && (
                  <span className="asv-msg-tokens">{msg.tokenUsage.prompt}\u2192{msg.tokenUsage.completion} tok</span>
                )}
              </div>

              {msg.segments && msg.segments.length > 0 ? (
                <>
                  {msg.segments.map((seg, si) =>
                    seg.kind === "text" ? (
                      <div key={si} className="asv-msg-content">
                        <Markdown>{seg.text}</Markdown>
                      </div>
                    ) : seg.kind === "tool" ? (
                      <div key={si} className="asv-tool-inline">
                        <span className="asv-tool-icon">
                          {seg.event.type === "start" ? "\u27F3" : seg.event.success ? "\u2713" : "\u2717"}
                        </span>
                        <span className="asv-tool-name">{seg.event.name}</span>
                        {seg.event.input_preview && <span className="asv-tool-input">{seg.event.input_preview}</span>}
                        {seg.event.duration_ms != null && <span className="asv-tool-ms">{formatMs(seg.event.duration_ms)}</span>}
                        {seg.event.output_preview && <ExpandableOutput text={seg.event.output_preview} />}
                      </div>
                    ) : seg.kind === "status" ? (
                      <div key={si} className="asv-status-item">{seg.text}</div>
                    ) : null
                  )}
                  {msg.role === "assistant" && <CopyButton text={msg.content} />}
                </>
              ) : (
                <>
                  <div className="asv-msg-content">
                    {msg.role === "assistant" ? <Markdown>{msg.content}</Markdown> : <span>{msg.content}</span>}
                  </div>
                  {msg.role === "assistant" && <CopyButton text={msg.content} />}
                </>
              )}
            </div>
          );
        })}

        {/* Live streaming */}
        {streaming && (
          <div className="asv-msg asv-msg-assistant asv-msg-streaming">
            <div className="asv-msg-header">
              <span className="asv-msg-role">assistant</span>
              {thinkingStart && <ThinkingTimer start={thinkingStart} />}
            </div>
            {streamText && (
              <div className="asv-msg-content"><Markdown>{streamText}</Markdown></div>
            )}
            {liveToolEvents.length > 0 && (
              <div className="asv-tool-live">
                <div className="asv-tool-live-header">
                  {liveToolEvents.some(e => e.type === "start") ? "working..." : `${liveToolEvents.filter(e => e.type === "complete").length} tool calls`}
                </div>
                {liveToolEvents.map((ev, i) =>
                  ev.type === "turn" ? (
                    <div key={i} className="asv-tool-live-item turn">
                      <span className="asv-tool-live-name">{ev.name}</span>
                    </div>
                  ) : ev.type === "status" ? (
                    <div key={i} className="asv-tool-live-item status">
                      <span className="asv-tool-live-name">{ev.name}</span>
                    </div>
                  ) : (
                    <div key={i} className={`asv-tool-live-item ${ev.type}`}>
                      <span className="asv-tool-icon">{ev.type === "start" ? "\u27F3" : ev.success ? "\u2713" : "\u2717"}</span>
                      <span className="asv-tool-live-name">{ev.name}</span>
                      {ev.duration_ms != null && <span className="asv-tool-ms">{formatMs(ev.duration_ms)}</span>}
                      {ev.type === "complete" && ev.output_preview && <ExpandableOutput text={ev.output_preview} />}
                    </div>
                  )
                )}
              </div>
            )}
            {!streamText && !liveToolEvents.length && <ThinkingStatus />}
            {liveToolEvents.some(e => e.type === "start") && (
              <ThinkingStatus toolName={liveToolEvents.filter(e => e.type === "start").pop()?.name} />
            )}
          </div>
        )}

        <div ref={messagesEnd} />
      </div>

      {/* Input box */}
      <div className="asv-composer">
        <div className={`asv-composer-inner ${streaming ? "asv-composer-busy" : ""}`}>
          <textarea
            ref={inputRef}
            className="asv-textarea"
            placeholder={streaming ? "Responding..." : `Message ${displayName}...`}
            value={input}
            onChange={handleInputChange}
            onKeyDown={handleKeyDown}
            disabled={streaming}
            rows={1}
          />
          <button
            className={`asv-send ${input.trim() && !streaming ? "ready" : ""} ${streaming ? "busy" : ""}`}
            onClick={handleSend}
            disabled={!input.trim() || streaming}
          >
            {streaming ? (
              <svg className="asv-send-spinner" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2">
                <circle cx="8" cy="8" r="6" strokeDasharray="28" strokeDashoffset="8" strokeLinecap="round" />
              </svg>
            ) : (
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M2 8h12M10 4l4 4-4 4" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
