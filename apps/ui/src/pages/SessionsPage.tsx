import { useEffect, useState, useRef, useCallback } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "@/lib/api";
import { useChatStore } from "@/store/chat";
import { useAuthStore } from "@/store/auth";

interface Message {
  role: string;
  content: string;
  timestamp?: string;
  source?: string;
}

interface StreamEvent {
  type: string;
  text?: string;
  delta?: string;
  message?: string;
  tool_name?: string;
  name?: string;
  input?: any;
  output?: string;
  result?: string;
  success?: boolean;
  cost_usd?: number;
  stop_reason?: string;
  event_type?: string;
  [key: string]: any;
}

export default function SessionsPage() {
  const [searchParams] = useSearchParams();
  const agentFilter = searchParams.get("agent");
  const selectedAgent = useChatStore((s) => s.selectedAgent);
  const token = useAuthStore((s) => s.token);
  const scope = agentFilter || selectedAgent;

  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [streamText, setStreamText] = useState("");
  const [activeTools, setActiveTools] = useState<string[]>([]);
  const messagesEnd = useRef<HTMLDivElement>(null);
  const wsRef = useRef<WebSocket | null>(null);

  // Load conversation history
  useEffect(() => {
    if (!scope) {
      // No agent selected — show overview
      setMessages([]);
      return;
    }
    api.getChatHistory({ project: undefined, limit: 30 })
      .then((d: any) => setMessages(d.messages || []))
      .catch(() => setMessages([]));
  }, [scope]);

  // Auto-scroll on new messages
  useEffect(() => {
    messagesEnd.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamText]);

  // Send message via WebSocket streaming
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
        const event: StreamEvent = JSON.parse(e.data);

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

          case "Status":
            // Status messages (task started, etc.)
            break;

          case "Complete":
            // Finalize the assistant message
            if (accumulated) {
              setMessages((prev) => [...prev, { role: "assistant", content: accumulated }]);
            }
            setStreamText("");
            setStreaming(false);
            setActiveTools([]);
            ws.close();
            break;

          case "Error":
            setMessages((prev) => [...prev, {
              role: "system",
              content: `Error: ${event.message || "Unknown error"}`,
            }]);
            setStreaming(false);
            ws.close();
            break;

          default:
            // TaskCompleted, TaskFailed, Progress, etc.
            if (event.event_type === "TaskCompleted" || event.event_type === "TaskFailed") {
              if (accumulated) {
                setMessages((prev) => [...prev, { role: "assistant", content: accumulated }]);
              }
              setStreamText("");
              setStreaming(false);
              ws.close();
            }
            break;
        }
      } catch {
        // ignore malformed
      }
    };

    ws.onerror = () => {
      setStreaming(false);
    };

    ws.onclose = () => {
      if (streaming) {
        // Unexpected close — save whatever we have
        if (accumulated) {
          setMessages((prev) => [...prev, { role: "assistant", content: accumulated }]);
          setStreamText("");
        }
        setStreaming(false);
      }
    };
  }, [input, streaming, token, scope]);

  // No agent selected — show overview
  if (!scope) {
    return (
      <div className="sessions-page">
        <div className="sessions-empty">
          Select an agent to start a session
        </div>
      </div>
    );
  }

  return (
    <div className="session-chat">
      <div className="session-chat-header">
        <span className="session-chat-name">{scope}</span>
        <span className="session-chat-status">
          {streaming ? "streaming" : "ready"}
        </span>
      </div>

      <div className="session-messages">
        {messages.map((msg, i) => (
          <div key={i} className={`session-msg session-msg-${msg.role}`}>
            <span className="session-msg-role">{msg.role}</span>
            <pre className="session-msg-content">{msg.content}</pre>
          </div>
        ))}

        {/* Live streaming text */}
        {streamText && (
          <div className="session-msg session-msg-assistant session-msg-streaming">
            <span className="session-msg-role">assistant</span>
            <pre className="session-msg-content">{streamText}</pre>
          </div>
        )}

        {/* Active tool indicators */}
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
          placeholder={streaming ? "Agent is responding..." : `Message ${scope}...`}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && handleSend()}
          disabled={streaming}
        />
      </div>
    </div>
  );
}
