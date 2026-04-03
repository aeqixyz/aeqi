import { useEffect, useState, useRef, useCallback } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "@/lib/api";
import { useChatStore } from "@/store/chat";
import { useAuthStore } from "@/store/auth";

interface Message {
  role: string;
  content: string;
  timestamp?: string;
}

interface SessionInfo {
  id: string;
  name: string;
  type: "perpetual" | "active" | "history";
  status?: string;
  agent?: string;
  skill?: string;
  time?: string;
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
  const scope = agentFilter || selectedAgent;

  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string>("perpetual");
  const [sessionCounter, setSessionCounter] = useState(0);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [streamText, setStreamText] = useState("");
  const [activeTools, setActiveTools] = useState<string[]>([]);
  const messagesEnd = useRef<HTMLDivElement>(null);
  const wsRef = useRef<WebSocket | null>(null);

  // Build session list from tasks + channels
  useEffect(() => {
    const list: SessionInfo[] = [];

    // Perpetual session is always first
    list.push({
      id: "perpetual",
      name: scope || "Assistant",
      type: "perpetual",
      status: "live",
    });

    // Fetch tasks for active/history sessions
    api.getTasks({}).then((d: any) => {
      const tasks = d.tasks || [];
      const filtered = scope
        ? tasks.filter((t: any) =>
            (t.assignee || t.agent_id || "").toLowerCase().includes(scope.toLowerCase())
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
      setSessions([...list]);
    }).catch(() => setSessions(list));
  }, [scope]);

  // Load messages for selected session
  useEffect(() => {
    if (activeSessionId === "perpetual") {
      api.getChatHistory({ limit: 50 })
        .then((d: any) => setMessages(d.messages || []))
        .catch(() => setMessages([]));
    } else if (activeSessionId.startsWith("new-")) {
      // Fresh session — empty
      setMessages([]);
    } else {
      // Existing task — try to load transcript
      setMessages([]);
    }
  }, [activeSessionId]);

  useEffect(() => {
    messagesEnd.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamText]);

  // Send message via WebSocket
  const handleSend = useCallback(() => {
    if (!input.trim() || streaming || !token) return;

    const userMsg: Message = { role: "user", content: input };
    setMessages((prev) => [...prev, userMsg]);
    setInput("");
    setStreaming(true);
    setStreamText("");
    setActiveTools([]);

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(
      `${protocol}//${window.location.host}/api/chat/stream?token=${token}`
    );
    wsRef.current = ws;

    ws.onopen = () => {
      ws.send(JSON.stringify({
        message: userMsg.content,
        agent: scope || undefined,
      }));
    };

    let accumulated = "";

    ws.onmessage = (e) => {
      try {
        const event = JSON.parse(e.data);
        switch (event.type) {
          case "TextDelta":
            accumulated += event.text || event.delta || "";
            setStreamText(accumulated);
            break;
          case "ToolCall":
          case "ToolStart":
            setActiveTools((prev) => [...prev, event.name || event.tool_name || "tool"]);
            break;
          case "ToolResult":
          case "ToolComplete":
            setActiveTools((prev) => prev.slice(1));
            break;
          case "Complete":
            if (accumulated) {
              setMessages((prev) => [...prev, { role: "assistant", content: accumulated }]);
            }
            setStreamText("");
            setStreaming(false);
            setActiveTools([]);
            ws.close();
            break;
          case "Error":
            setMessages((prev) => [...prev, { role: "system", content: `Error: ${event.message}` }]);
            setStreaming(false);
            ws.close();
            break;
          default:
            if (event.event_type === "TaskCompleted" || event.event_type === "TaskFailed") {
              if (accumulated) {
                setMessages((prev) => [...prev, { role: "assistant", content: accumulated }]);
              }
              setStreamText("");
              setStreaming(false);
              ws.close();
            }
        }
      } catch {}
    };

    ws.onerror = () => setStreaming(false);
    ws.onclose = () => {
      if (accumulated && streaming) {
        setMessages((prev) => [...prev, { role: "assistant", content: accumulated }]);
        setStreamText("");
      }
      setStreaming(false);
    };
  }, [input, streaming, token, scope]);

  if (!scope) {
    return (
      <div className="sessions-page">
        <div className="sessions-empty">Select an agent to view sessions</div>
      </div>
    );
  }

  const activeSessions = sessions.filter((s) => s.type === "active");
  const historySessions = sessions.filter((s) => s.type === "history");

  return (
    <div className="sessions-split">
      {/* Session list */}
      <div className="sessions-list-pane">
        <div className="sessions-list-section">
          {sessions.filter((s) => s.type === "perpetual").map((s) => (
            <div
              key={s.id}
              className={`session-list-item${activeSessionId === s.id ? " active" : ""}`}
              onClick={() => setActiveSessionId(s.id)}
            >
              <span className="session-list-dot">●</span>
              <span className="session-list-name">{s.name}</span>
            </div>
          ))}
        </div>

        {activeSessions.length > 0 && (
          <div className="sessions-list-section">
            <div className="sessions-list-header">active</div>
            {activeSessions.map((s) => (
              <div
                key={s.id}
                className={`session-list-item${activeSessionId === s.id ? " active" : ""}`}
                onClick={() => setActiveSessionId(s.id)}
              >
                <span className="session-list-dot">●</span>
                <span className="session-list-name">{s.name}</span>
                {s.time && <span className="session-list-time">{timeAgo(s.time)}</span>}
              </div>
            ))}
          </div>
        )}

        <div className="session-list-add" onClick={() => {
          const id = `new-${Date.now()}`;
          const num = sessionCounter + 1;
          setSessionCounter(num);
          setSessions((prev) => [
            ...prev.filter((s) => s.type !== "history"),
            {
              id,
              name: `session ${num}`,
              type: "active" as const,
              status: "new",
            },
            ...prev.filter((s) => s.type === "history"),
          ]);
          setActiveSessionId(id);
          setMessages([]);
          setStreamText("");
        }}>+</div>

        {historySessions.length > 0 && (
          <div className="sessions-list-section">
            <div className="sessions-list-header">history</div>
            {historySessions.map((s) => (
              <div
                key={s.id}
                className={`session-list-item${activeSessionId === s.id ? " active" : ""}`}
                onClick={() => setActiveSessionId(s.id)}
              >
                <span className="session-list-dot dim">○</span>
                <span className="session-list-name">{s.name}</span>
                {s.time && <span className="session-list-time">{timeAgo(s.time)}</span>}
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Session transcript */}
      <div className="sessions-transcript-pane">
        <div className="session-messages">
          {messages.map((msg, i) => (
            <div key={i} className={`session-msg session-msg-${msg.role}`}>
              <span className="session-msg-role">{msg.role}</span>
              <pre className="session-msg-content">{msg.content}</pre>
            </div>
          ))}

          {streamText && (
            <div className="session-msg session-msg-assistant session-msg-streaming">
              <span className="session-msg-role">assistant</span>
              <pre className="session-msg-content">{streamText}</pre>
            </div>
          )}

          {activeTools.length > 0 && (
            <div className="session-tool-indicator">
              {activeTools.map((tool, i) => (
                <span key={i} className="session-tool-name">⟳ {tool}</span>
              ))}
            </div>
          )}

          <div ref={messagesEnd} />
        </div>

        <div className="session-input-wrap">
          <input
            className="session-input"
            type="text"
            placeholder={streaming ? "Responding..." : `Message ${scope}...`}
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
